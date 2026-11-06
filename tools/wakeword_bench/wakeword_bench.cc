// Wake word benchmark for the NSPanel Pro (development tool).
//
// Runs microWakeWord streaming models (the same .tflite models ESPHome uses
// on voice satellites) with TensorFlow Lite on the panel, to measure the CPU
// cost of on-device wake word detection and to try detection with the
// panel's own microphones.
//
// Modes:
//   wakeword_bench synth <steps> <model.tflite>...
//       Random input, measures the time per inference of each model and the
//       resulting load at real-time rate (one inference per model stride).
//   wakeword_bench live [options] <model.tflite[:cutoff[:window]]>...
//       Reads interleaved S16_LE PCM at 16 kHz from stdin (e.g. arecord),
//       computes the microWakeWord features (TFLite micro frontend with the
//       ESPHome settings), runs every model and prints detections, a
//       per-second probability summary and the CPU load.
//       Options: --channels N (default 1), --channel I (default 0),
//                --gain G (linear, default 1), --seconds S (0 = until EOF)
//
// Feature settings and the int8 quantisation follow ESPHome's
// micro_wake_word component (preprocessor_settings.h, streaming_model.cpp).

#include <sys/resource.h>
#include <time.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <random>
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
constexpr int kSamplesPerStep = kSampleRate * kStepMs / 1000;

double now_s() {
  timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return ts.tv_sec + ts.tv_nsec * 1e-9;
}

double cpu_s() {
  rusage ru;
  getrusage(RUSAGE_SELF, &ru);
  return ru.ru_utime.tv_sec + ru.ru_utime.tv_usec * 1e-6 + ru.ru_stime.tv_sec +
         ru.ru_stime.tv_usec * 1e-6;
}

// Whole-system CPU usage from /proc/stat (all cores together).
struct SysCpu {
  unsigned long long busy = 0, total = 0;
  void read() {
    FILE* f = fopen("/proc/stat", "r");
    if (!f) return;
    unsigned long long v[10] = {0};
    if (fscanf(f, "cpu %llu %llu %llu %llu %llu %llu %llu %llu %llu %llu", &v[0], &v[1],
               &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9]) >= 4) {
      total = 0;
      for (auto x : v) total += x;
      busy = total - v[3] - v[4];
    }
    fclose(f);
  }
};

std::string read_line(const char* path) {
  FILE* f = fopen(path, "r");
  if (!f) return "?";
  char buf[128] = {0};
  if (!fgets(buf, sizeof buf, f)) buf[0] = 0;
  fclose(f);
  std::string s(buf);
  while (!s.empty() && (s.back() == '\n' || s.back() == ' ')) s.pop_back();
  return s;
}

struct Model {
  std::string path, name;
  float cutoff = 0.97f;
  int window = 5;
  std::unique_ptr<tflite::FlatBufferModel> fb;
  std::unique_ptr<tflite::Interpreter> interp;
  int stride = 1;
  std::vector<uint8_t> recent;  // last `window` probabilities
  double invoke_s = 0;
  long invokes = 0;
  int max_prob_this_second = 0;
  int refractory = 0;  // model steps to ignore after a detection

  bool load() {
    fb = tflite::FlatBufferModel::BuildFromFile(path.c_str());
    if (!fb) return false;
    tflite::ops::builtin::BuiltinOpResolverWithoutDefaultDelegates resolver;
    tflite::InterpreterBuilder builder(*fb, resolver);
    builder.SetNumThreads(1);
    if (builder(&interp) != kTfLiteOk || !interp) return false;
    if (interp->AllocateTensors() != kTfLiteOk) return false;
    TfLiteTensor* in = interp->input_tensor(0);
    if (in->type != kTfLiteInt8 || in->dims->size != 3 || in->dims->data[2] != kFeatureSize) {
      fprintf(stderr, "%s: unexpected input tensor\n", path.c_str());
      return false;
    }
    stride = in->dims->data[1];
    TfLiteTensor* out = interp->output_tensor(0);
    if (out->type != kTfLiteUInt8) {
      fprintf(stderr, "%s: unexpected output type\n", path.c_str());
      return false;
    }
    size_t slash = path.find_last_of('/');
    name = path.substr(slash == std::string::npos ? 0 : slash + 1);
    return true;
  }

  // Runs one inference on `stride` feature frames; returns the probability (0..255).
  int run(const int8_t* frames) {
    std::memcpy(interp->typed_input_tensor<int8_t>(0), frames, stride * kFeatureSize);
    double t0 = now_s();
    if (interp->Invoke() != kTfLiteOk) {
      fprintf(stderr, "%s: invoke failed\n", name.c_str());
      exit(1);
    }
    invoke_s += now_s() - t0;
    invokes++;
    return interp->typed_output_tensor<uint8_t>(0)[0];
  }
};

int8_t quantise(uint16_t feature) {
  // ESPHome: input = (feature * 256) / (25.6 * 26.0) - 128, in integer math.
  constexpr int32_t kScale = 256, kDiv = 666;
  int32_t v = ((static_cast<int32_t>(feature) * kScale) + (kDiv / 2)) / kDiv - 128;
  return static_cast<int8_t>(std::clamp(v, -128, 127));
}

int synth(long steps, std::vector<Model>& models) {
  std::mt19937 rng(1);
  std::uniform_int_distribution<int> dist(-128, 127);
  std::vector<int8_t> buf(16 * kFeatureSize);
  printf("cpu0 governor %s, cur %s kHz\n",
         read_line("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").c_str(),
         read_line("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").c_str());
  double total_load = 0;
  for (auto& m : models) {
    for (int i = 0; i < 50; i++) {  // warm-up
      for (auto& b : buf) b = static_cast<int8_t>(dist(rng));
      m.run(buf.data());
    }
    m.invoke_s = 0;
    m.invokes = 0;
    for (long i = 0; i < steps; i++) {
      for (int j = 0; j < m.stride * kFeatureSize; j++) buf[j] = static_cast<int8_t>(dist(rng));
      m.run(buf.data());
    }
    double us = m.invoke_s / m.invokes * 1e6;
    double period_us = m.stride * kStepMs * 1000.0;
    double load = us / period_us * 100.0;
    total_load += load;
    printf("%-20s stride %d: %8.1f us per inference, one every %.0f ms -> %5.2f %% of one core\n",
           m.name.c_str(), m.stride, us, period_us / 1000.0, load);
  }
  printf("all models together: %.2f %% of one core (= %.2f %% of the 4-core CPU)\n", total_load,
         total_load / 4.0);
  return 0;
}

int live(int channels, int channel, float gain, double seconds, std::vector<Model>& models) {
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
  FrontendState state;
  if (!FrontendPopulateState(&config, &state, kSampleRate)) {
    fprintf(stderr, "frontend init failed\n");
    return 1;
  }

  std::vector<int16_t> frame(kSamplesPerStep * channels);
  std::vector<int16_t> mono(kSamplesPerStep);
  // Feature history per model (frames not yet fed).
  std::vector<std::vector<int8_t>> pending(models.size());
  double frontend_s = 0;
  long steps = 0;
  double t_start = now_s(), c_start = cpu_s();
  double t_report = t_start, c_report = c_start;
  SysCpu sys0, sys1;
  sys0.read();
  double peak_dbfs = -200;

  while (true) {
    size_t got = fread(frame.data(), sizeof(int16_t) * channels, kSamplesPerStep, stdin);
    if (got < static_cast<size_t>(kSamplesPerStep)) break;
    double sq = 0;
    for (int i = 0; i < kSamplesPerStep; i++) {
      float v = frame[i * channels + channel] * gain;
      v = std::clamp(v, -32768.0f, 32767.0f);
      mono[i] = static_cast<int16_t>(v);
      sq += static_cast<double>(mono[i]) * mono[i];
    }
    double rms = std::sqrt(sq / kSamplesPerStep);
    peak_dbfs = std::max(peak_dbfs, 20 * std::log10(std::max(rms, 0.5) / 32768.0));

    double f0 = now_s();
    size_t used = 0;
    const int16_t* p = mono.data();
    size_t left = mono.size();
    std::vector<int8_t> features;
    while (left > 0) {
      FrontendOutput out = FrontendProcessSamples(&state, p, left, &used);
      p += used;
      left -= used;
      if (out.values && out.size == static_cast<size_t>(kFeatureSize)) {
        for (int i = 0; i < kFeatureSize; i++) features.push_back(quantise(out.values[i]));
      }
      if (used == 0) break;
    }
    frontend_s += now_s() - f0;
    steps++;

    for (size_t mi = 0; mi < models.size(); mi++) {
      Model& m = models[mi];
      auto& pend = pending[mi];
      pend.insert(pend.end(), features.begin(), features.end());
      while (pend.size() >= static_cast<size_t>(m.stride * kFeatureSize)) {
        int prob = m.run(pend.data());
        pend.erase(pend.begin(), pend.begin() + m.stride * kFeatureSize);
        m.max_prob_this_second = std::max(m.max_prob_this_second, prob);
        m.recent.push_back(static_cast<uint8_t>(prob));
        if (m.recent.size() > static_cast<size_t>(m.window)) m.recent.erase(m.recent.begin());
        if (m.refractory > 0) {
          m.refractory--;
          continue;
        }
        if (m.recent.size() == static_cast<size_t>(m.window)) {
          int sum = 0;
          for (auto r : m.recent) sum += r;
          if (sum > m.cutoff * 255.0f * m.window) {
            printf("%8.2f s  DETECTED %s (mean %.3f)\n", now_s() - t_start, m.name.c_str(),
                   sum / (255.0 * m.window));
            fflush(stdout);
            m.recent.clear();
            m.refractory = 1000 / (m.stride * kStepMs);  // 1 s
          }
        }
      }
    }

    double t = now_s();
    if (t - t_report >= 1.0) {
      double c = cpu_s();
      printf("%8.2f s  level %6.1f dBFS  cpu %5.1f %%", t - t_start, peak_dbfs,
             (c - c_report) / (t - t_report) * 100.0);
      for (auto& m : models) {
        printf("  %s %.2f", m.name.c_str(), m.max_prob_this_second / 255.0);
        m.max_prob_this_second = 0;
      }
      printf("\n");
      fflush(stdout);
      t_report = t;
      c_report = c;
      peak_dbfs = -200;
    }
    if (seconds > 0 && t - t_start >= seconds) break;
  }

  double wall = now_s() - t_start, cpu = cpu_s() - c_start;
  sys1.read();
  printf("\nsummary: %.1f s audio, %ld feature steps\n", steps * kStepMs / 1000.0, steps);
  printf("  frontend: %.1f us per 10 ms step (%.2f %% of one core)\n", frontend_s / steps * 1e6,
         frontend_s / (steps * kStepMs / 1000.0) * 100.0);
  for (auto& m : models) {
    if (m.invokes == 0) continue;
    printf("  %-20s %.1f us per inference, %ld inferences (%.2f %% of one core)\n", m.name.c_str(),
           m.invoke_s / m.invokes * 1e6, m.invokes,
           m.invoke_s / (steps * kStepMs / 1000.0) * 100.0);
  }
  printf("  this process: %.2f %% of one core over %.1f s wall time\n", cpu / wall * 100.0, wall);
  if (sys1.total > sys0.total) {
    printf("  whole system (4 cores, incl. arecord and everything else): %.2f %% busy\n",
           100.0 * (sys1.busy - sys0.busy) / (sys1.total - sys0.total));
  }
  FrontendFreeStateContents(&state);
  return 0;
}

}  // namespace

int main(int argc, char** argv) {
  if (argc < 3) {
    fprintf(stderr,
            "usage: %s synth <steps> <model.tflite>...\n"
            "       %s live [--channels N] [--channel I] [--gain G] [--seconds S] "
            "<model.tflite[:cutoff[:window]]>...\n",
            argv[0], argv[0]);
    return 2;
  }
  std::string mode = argv[1];
  int channels = 1, channel = 0;
  float gain = 1.0f;
  double seconds = 0;
  long steps = 0;
  int i = 2;
  if (mode == "synth") {
    steps = atol(argv[i++]);
  } else if (mode == "live") {
    for (; i < argc && strncmp(argv[i], "--", 2) == 0; i += 2) {
      if (i + 1 >= argc) return 2;
      std::string opt = argv[i];
      if (opt == "--channels") channels = atoi(argv[i + 1]);
      else if (opt == "--channel") channel = atoi(argv[i + 1]);
      else if (opt == "--gain") gain = atof(argv[i + 1]);
      else if (opt == "--seconds") seconds = atof(argv[i + 1]);
      else return 2;
    }
  } else {
    return 2;
  }
  std::vector<Model> models;
  for (; i < argc; i++) {
    Model m;
    std::string spec = argv[i];
    size_t c1 = spec.find(':');
    m.path = spec.substr(0, c1);
    if (c1 != std::string::npos) {
      size_t c2 = spec.find(':', c1 + 1);
      m.cutoff = atof(spec.substr(c1 + 1, c2 - c1 - 1).c_str());
      if (c2 != std::string::npos) m.window = atoi(spec.substr(c2 + 1).c_str());
    }
    if (!m.load()) {
      fprintf(stderr, "cannot load %s\n", m.path.c_str());
      return 1;
    }
    printf("loaded %s: stride %d, cutoff %.2f, window %d\n", m.name.c_str(), m.stride, m.cutoff,
           m.window);
    models.push_back(std::move(m));
  }
  if (models.empty()) return 2;
  return mode == "synth" ? synth(steps, models) : live(channels, channel, gain, seconds, models);
}
