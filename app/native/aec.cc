// Echo cancellation for the voice satellite (C API for Rust).
//
// WebRTC AudioProcessing (webrtc-audio-processing 1.3, Buildroot package):
// AEC3 echo canceller + high-pass filter (+ optional noise suppression) on
// 16 kHz mono, 10 ms frames. The far-end (render) signal is what the panel's
// own player sends to the speaker, so the microphones can keep listening for
// the wake word while the panel talks or beeps.

#include <cstdint>

#include "modules/audio_processing/include/audio_processing.h"

struct AecEngine {
  webrtc::AudioProcessing* apm = nullptr;
  webrtc::StreamConfig config{16000, 1};
};

extern "C" {

// noise_suppression: -1 off, 0 low, 1 moderate, 2 high, 3 very high.
AecEngine* aec_create(int noise_suppression) {
  auto* e = new AecEngine();
  e->apm = webrtc::AudioProcessingBuilder().Create();
  if (!e->apm) {
    delete e;
    return nullptr;
  }
  webrtc::AudioProcessing::Config c;
  c.echo_canceller.enabled = true;
  c.echo_canceller.mobile_mode = false;
  c.high_pass_filter.enabled = true;
  c.noise_suppression.enabled = noise_suppression >= 0;
  switch (noise_suppression) {
    case 0: c.noise_suppression.level = webrtc::AudioProcessing::Config::NoiseSuppression::kLow; break;
    case 2: c.noise_suppression.level = webrtc::AudioProcessing::Config::NoiseSuppression::kHigh; break;
    case 3: c.noise_suppression.level = webrtc::AudioProcessing::Config::NoiseSuppression::kVeryHigh; break;
    default: c.noise_suppression.level = webrtc::AudioProcessing::Config::NoiseSuppression::kModerate; break;
  }
  e->apm->ApplyConfig(c);
  return e;
}

// One 10 ms frame (160 samples): far = what the speaker plays now (zeros when
// silent), near = microphones; out may be near. Returns 0 on success.
int aec_process(AecEngine* e, const int16_t* far, const int16_t* near, int16_t* out) {
  int16_t far_out[160];
  e->apm->ProcessReverseStream(far, e->config, e->config, far_out);
  e->apm->set_stream_delay_ms(0);
  return e->apm->ProcessStream(near, e->config, e->config, out);
}

}  // extern "C"
