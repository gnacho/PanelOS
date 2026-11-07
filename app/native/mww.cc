// On-device wake word detection for the panel app (C API for Rust).
//
// microWakeWord streaming models (the .tflite files ESPHome voice satellites
// use) run with TensorFlow Lite; features come from the TFLite micro frontend
// with ESPHome's micro_wake_word settings (preprocessor_settings.h) and its
// int8 quantisation (streaming_model.cpp). Measured on the panel: about 5 %
// of one core for two wake words plus the frontend (docs/bringup.md).

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

#include "tensorflow/lite/interpreter.h"
#include "tensorflow/lite/kernels/register.h"
#include "tensorflow/lite/model.h"

extern "C" {
#include "tensorflow/lite/experimental/microfrontend/lib/frontend.h"
#include "tensorflow/lite/experimental/microfrontend/lib/frontend_util.h"
}

namespace {

constexpr int kSampleRate = 16000;
constexpr int kFeatureSize = 40;
constexpr int kStepMs = 10;

struct Model {
  std::unique_ptr<tflite::FlatBufferModel> fb;
  std::unique_ptr<tflite::Interpreter> interp;
  int stride = 1;
  float cutoff = 0.97f;
  int window = 5;
  bool enabled = true;
  std::vector<int8_t> pending;
  std::vector<uint8_t> recent;
  int refractory = 0;
};

int8_t quantise(uint16_t feature) {
  constexpr int32_t kScale = 256, kDiv = 666;
  int32_t v = ((static_cast<int32_t>(feature) * kScale) + (kDiv / 2)) / kDiv - 128;
  return static_cast<int8_t>(std::clamp(v, -128, 127));
}

}  // namespace

struct MwwEngine {
  FrontendState frontend;
  std::vector<Model> models;
  float last_score = 0.0f;  // window-averaged probability of the last detection, 0..1
};

extern "C" {

MwwEngine* mww_create() {
  auto* e = new MwwEngine();
  FrontendConfig config;
  FrontendFillConfigWithDefaults(&config);
  config.window.size_ms = 30;
  config.window.step_size_ms = kStepMs;
  config.filterbank.num_channels = kFeatureSize;
  config.filterbank.lower_band_limit = 125.0f;
  config.filterbank.upper_band_limit = 7500.0f;
  config.noise_reduction.smoothing_bits = 10;
  config.noise_reduction.even_smoothing = 0.025f;
  config.noise_reduction.odd_smoothing = 0.06f;
  config.noise_reduction.min_signal_remaining = 0.05f;
  config.pcan_gain_control.enable_pcan = 1;
  config.pcan_gain_control.strength = 0.95f;
  config.pcan_gain_control.offset = 80.0f;
  config.pcan_gain_control.gain_bits = 21;
  config.log_scale.enable_log = 1;
  config.log_scale.scale_shift = 6;
  if (!FrontendPopulateState(&config, &e->frontend, kSampleRate)) {
    delete e;
    return nullptr;
  }
  return e;
}

// Adds a model; returns its index or -1.
int mww_add_model(MwwEngine* e, const char* path, float cutoff, int window) {
  Model m;
  m.fb = tflite::FlatBufferModel::BuildFromFile(path);
  if (!m.fb) return -1;
  tflite::ops::builtin::BuiltinOpResolverWithoutDefaultDelegates resolver;
  tflite::InterpreterBuilder builder(*m.fb, resolver);
  builder.SetNumThreads(1);
  if (builder(&m.interp) != kTfLiteOk || !m.interp) return -1;
  if (m.interp->AllocateTensors() != kTfLiteOk) return -1;
  TfLiteTensor* in = m.interp->input_tensor(0);
  if (in->type != kTfLiteInt8 || in->dims->size != 3 || in->dims->data[2] != kFeatureSize) return -1;
  if (m.interp->output_tensor(0)->type != kTfLiteUInt8) return -1;
  m.stride = in->dims->data[1];
  m.cutoff = cutoff;
  m.window = std::max(1, window);
  e->models.push_back(std::move(m));
  return static_cast<int>(e->models.size()) - 1;
}

static void flush(Model& m);

// Disabled models are not run at all (CPU); a model that is switched on is
// flushed first, so it starts from silence instead of stale audio.
void mww_set_enabled(MwwEngine* e, int index, int enabled) {
  if (index >= 0 && index < static_cast<int>(e->models.size())) {
    Model& m = e->models[index];
    bool on = enabled != 0;
    if (on && !m.enabled) flush(m);
    m.enabled = on;
    m.recent.clear();
    m.pending.clear();
  }
}

// Forget recent probabilities (after a conversation, so old audio does not trigger).
//
// The streaming models keep their recent input inside the model (resource
// variables), so clearing the probabilities is not enough: after a
// conversation the model would still hold the wake word that started it and
// fire again within ~0.2 s of listening (a new run 0.18 s after the
// previous one, then 15 s of needless listening). Feeding silence
// pushes the old audio out of the model's buffers.
static void flush(Model& m) {
  constexpr int kFlushFrames = 300;  // 3 s of 10 ms feature frames, more than any model's receptive field
  int8_t* in = m.interp->typed_input_tensor<int8_t>(0);
  for (int i = 0; i < kFlushFrames / m.stride; i++) {
    std::memset(in, -128, static_cast<size_t>(m.stride * kFeatureSize));
    m.interp->Invoke();
  }
  m.recent.clear();
  m.pending.clear();
  m.refractory = 0;
}

void mww_reset(MwwEngine* e) {
  for (auto& m : e->models) {
    if (m.enabled) flush(m);
  }
}

// Feeds 16 kHz mono PCM; returns the index of a detected model or -1.
int mww_process(MwwEngine* e, const int16_t* samples, size_t count) {
  int detected = -1;
  while (count > 0) {
    size_t used = 0;
    FrontendOutput out = FrontendProcessSamples(&e->frontend, samples, count, &used);
    samples += used;
    count -= used;
    if (out.values && out.size == static_cast<size_t>(kFeatureSize)) {
      for (auto& m : e->models) {
        if (!m.enabled) continue;
        for (int i = 0; i < kFeatureSize; i++) m.pending.push_back(quantise(out.values[i]));
        while (m.pending.size() >= static_cast<size_t>(m.stride * kFeatureSize)) {
          std::memcpy(m.interp->typed_input_tensor<int8_t>(0), m.pending.data(), m.stride * kFeatureSize);
          m.pending.erase(m.pending.begin(), m.pending.begin() + m.stride * kFeatureSize);
          if (m.interp->Invoke() != kTfLiteOk) continue;
          uint8_t prob = m.interp->typed_output_tensor<uint8_t>(0)[0];
          m.recent.push_back(prob);
          if (m.recent.size() > static_cast<size_t>(m.window)) m.recent.erase(m.recent.begin());
          if (m.refractory > 0) {
            m.refractory--;
            continue;
          }
          if (m.recent.size() == static_cast<size_t>(m.window)) {
            int sum = 0;
            for (auto r : m.recent) sum += r;
            if (sum > m.cutoff * 255.0f * m.window && detected < 0) {
              detected = static_cast<int>(&m - e->models.data());
              e->last_score = sum / (255.0f * m.window);
              m.recent.clear();
              m.refractory = 1000 / (m.stride * kStepMs);
            }
          }
        }
      }
    }
    if (used == 0) break;
  }
  return detected;
}

// Window-averaged probability (0..1) of the last detection.
float mww_last_score(MwwEngine* e) { return e ? e->last_score : 0.0f; }

void mww_destroy(MwwEngine* e) {
  if (!e) return;
  FrontendFreeStateContents(&e->frontend);
  delete e;
}

}  // extern "C"
