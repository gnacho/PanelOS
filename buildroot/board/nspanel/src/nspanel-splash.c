// nspanel-splash: boot animation of the NSPanel Pro open firmware.
//
// Plays splash.bin (made by tools/splash/storyboard.py from assets/logo.svg
// and assets/logotype.svg, pixel-exact) straight to the display through a DRM
// dumb buffer, from the first second of user space: the logotype, the house
// gliding into the round logo, the line drawing itself, the text under it.
//
// Hand-over: the panel app sends SIGUSR1 when it wants the display. The
// splash first finishes its sequence and keeps the finished picture for at
// least MIN_HOLD (the UI must not cut the animation short). Then it waits
// for Wi-Fi (a default route on wlan0; the panel is no use without the
// network, so the UI may wait for it): meanwhile the
// small logotype dims and a light band runs across it. When the app has asked
// and Wi-Fi is up, or CAP seconds after the start without Wi-Fi, it fades back
// to the finished picture, gives up DRM master, keeps the picture and creates
// DONE_FILE; the app then opens the display and shows the same picture as its
// first frame. SIGTERM: exit.
//
// splash.bin v3: "NSPA", u16 version (3), u16 fps, u16 sequence records,
// u16 done record, u16 fade-in records, u16 loop records, u16 fade-out
// records; then the records in that order, each u16 hold (ticks of 1/fps),
// u32 bytes, then spans of changed pixels against the previous picture:
// u16 y, u16 x, u16 n, n x RGB. The loop starts and ends on the same (dimmed)
// picture, the fade-out ends on the finished picture.
//
// Static binary, no libdrm: plain DRM ioctls.
// SPDX-License-Identifier: AGPL-3.0-or-later

#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#include <drm/drm.h>
#include <drm/drm_mode.h>

#define DONE_FILE "/var/run/nspanel-splash.done"
#define MIN_HOLD 1.0 /* seconds the finished picture stays before the hand-over */
#define CAP 30.0     /* seconds from the start: hand over even without Wi-Fi */
#define GIVE_UP 300.0 /* seconds from the start without a request from the app */

static volatile sig_atomic_t handoff, quit;
static void on_usr1(int s) { (void)s; handoff = 1; }
static void on_term(int s) { (void)s; quit = 1; }

static uint32_t *fb;
static uint32_t pitch_px, fb_w, fb_h;

struct rec {
	const uint8_t *p;
	uint32_t len;
	unsigned hold;
};

static uint16_t rd16(const uint8_t *p) { return (uint16_t)(p[0] | p[1] << 8); }
static uint32_t rd32(const uint8_t *p) { return p[0] | p[1] << 8 | p[2] << 16 | (uint32_t)p[3] << 24; }

static int drm_setup(int fd)
{
	uint32_t conns[8], crtcs[8], encs[8];
	struct drm_mode_card_res res = {0};
	if (ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res) || res.count_connectors > 8 || res.count_crtcs > 8 ||
	    res.count_encoders > 8 || res.count_crtcs == 0)
		return -1;
	res.count_fbs = 0;
	res.connector_id_ptr = (uintptr_t)conns;
	res.crtc_id_ptr = (uintptr_t)crtcs;
	res.encoder_id_ptr = (uintptr_t)encs;
	if (ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res))
		return -1;
	for (uint32_t i = 0; i < res.count_connectors; i++) {
		struct drm_mode_modeinfo modes[16];
		uint32_t enc_ids[8], props[64];
		uint64_t values[64];
		struct drm_mode_get_connector c = {0};
		c.connector_id = conns[i];
		if (ioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &c) || c.connection != 1 || c.count_modes == 0)
			continue;
		if (c.count_modes > 16) c.count_modes = 16;
		if (c.count_encoders > 8) c.count_encoders = 8;
		if (c.count_props > 64) c.count_props = 64;
		c.modes_ptr = (uintptr_t)modes;
		c.encoders_ptr = (uintptr_t)enc_ids;
		c.props_ptr = (uintptr_t)props;
		c.prop_values_ptr = (uintptr_t)values;
		if (ioctl(fd, DRM_IOCTL_MODE_GETCONNECTOR, &c))
			continue;
		struct drm_mode_modeinfo *m = &modes[0];
		for (uint32_t k = 0; k < c.count_modes; k++)
			if (modes[k].type & DRM_MODE_TYPE_PREFERRED)
				m = &modes[k];
		uint32_t crtc = crtcs[0];
		if (c.encoder_id) {
			struct drm_mode_get_encoder e = {0};
			e.encoder_id = c.encoder_id;
			if (!ioctl(fd, DRM_IOCTL_MODE_GETENCODER, &e) && e.crtc_id)
				crtc = e.crtc_id;
		}
		struct drm_mode_create_dumb cd = {0};
		cd.width = m->hdisplay;
		cd.height = m->vdisplay;
		cd.bpp = 32;
		if (ioctl(fd, DRM_IOCTL_MODE_CREATE_DUMB, &cd))
			return -1;
		struct drm_mode_fb_cmd f = {0};
		f.width = cd.width;
		f.height = cd.height;
		f.pitch = cd.pitch;
		f.bpp = 32;
		f.depth = 24;
		f.handle = cd.handle;
		if (ioctl(fd, DRM_IOCTL_MODE_ADDFB, &f))
			return -1;
		struct drm_mode_map_dumb md = {0};
		md.handle = cd.handle;
		if (ioctl(fd, DRM_IOCTL_MODE_MAP_DUMB, &md))
			return -1;
		fb = mmap(NULL, cd.size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, md.offset);
		if (fb == MAP_FAILED)
			return -1;
		memset(fb, 0, cd.size);
		pitch_px = cd.pitch / 4;
		fb_w = cd.width;
		fb_h = cd.height;
		struct drm_mode_crtc sc = {0};
		sc.crtc_id = crtc;
		sc.fb_id = f.fb_id;
		sc.set_connectors_ptr = (uintptr_t)&conns[i];
		sc.count_connectors = 1;
		sc.mode = *m;
		sc.mode_valid = 1;
		return ioctl(fd, DRM_IOCTL_MODE_SETCRTC, &sc) ? -1 : 0;
	}
	return -1;
}

// Checks one record's spans against the display (draw = 0) or draws them.
static int spans(const struct rec *r, int draw)
{
	const uint8_t *p = r->p, *end = r->p + r->len;
	while (p + 6 <= end) {
		uint32_t y = rd16(p), x = rd16(p + 2), n = rd16(p + 4);
		p += 6;
		if (p + 3 * n > end || y >= fb_h || x + n > fb_w)
			return -1;
		if (draw) {
			uint32_t *row = fb + (size_t)y * pitch_px + x;
			for (uint32_t i = 0; i < n; i++)
				row[i] = (uint32_t)p[3 * i] << 16 | (uint32_t)p[3 * i + 1] << 8 | p[3 * i + 2];
		}
		p += 3 * n;
	}
	return p == end ? 0 : -1;
}

static double now(void)
{
	struct timespec ts;
	clock_gettime(CLOCK_MONOTONIC, &ts);
	return ts.tv_sec + ts.tv_nsec / 1e9;
}

static void wait_until(double t)
{
	// a signal (the app's SIGUSR1) cuts nanosleep short: sleep the rest
	for (double d; !quit && (d = t - now()) > 0;) {
		struct timespec ts = {(time_t)d, (long)((d - (time_t)d) * 1e9)};
		nanosleep(&ts, NULL);
	}
}

static unsigned fps;
static double tick; // time of the next picture

static void play(const struct rec *r, unsigned n)
{
	for (unsigned i = 0; i < n && !quit; i++) {
		spans(&r[i], 1);
		wait_until(tick += (double)(r[i].hold ? r[i].hold : 1) / fps);
	}
}

static int wifi_up(void)
{
	char line[256];
	int up = 0;
	FILE *f = fopen("/proc/net/route", "r");
	if (!f)
		return 0;
	while (!up && fgets(line, sizeof line, f))
		up = !strncmp(line, "wlan0\t00000000\t", 15);
	fclose(f);
	return up;
}

static void klog(const char *msg, double t)
{
	char buf[128];
	int k = open("/dev/kmsg", O_WRONLY | O_CLOEXEC);
	if (k < 0)
		return;
	int n = snprintf(buf, sizeof buf, "nspanel-splash: %s after %.1f s\n", msg, t);
	if (n > 0 && write(k, buf, (size_t)n) < 0) {
		/* nothing to do */
	}
	close(k);
}

int main(int argc, char **argv)
{
	const double t0 = now();
	const char *path = argc > 1 ? argv[1] : "/usr/share/nspanel/splash.bin";
	int df = open(path, O_RDONLY);
	struct stat st;
	if (df < 0 || fstat(df, &st) || st.st_size < 18) {
		fprintf(stderr, "nspanel-splash: cannot read %s\n", path);
		return 1;
	}
	const uint8_t *data = mmap(NULL, st.st_size, PROT_READ, MAP_PRIVATE, df, 0);
	if (data == MAP_FAILED || memcmp(data, "NSPA", 4) || rd16(data + 4) != 3) {
		fprintf(stderr, "nspanel-splash: bad data file %s\n", path);
		return 1;
	}
	fps = rd16(data + 6) ? rd16(data + 6) : 30;
	unsigned n_seq = rd16(data + 8), n_in = rd16(data + 12), n_loop = rd16(data + 14), n_out = rd16(data + 16);
	unsigned total = n_seq + n_in + n_loop + n_out;
	struct rec *recs = calloc(total ? total : 1, sizeof *recs);
	const uint8_t *p = data + 18, *end = data + st.st_size;
	unsigned got = 0;
	for (; got < total && p + 6 <= end; got++) {
		recs[got].hold = rd16(p);
		recs[got].len = rd32(p + 2);
		recs[got].p = p + 6;
		if (recs[got].len > (size_t)(end - p - 6))
			break;
		p += 6 + recs[got].len;
	}

	signal(SIGUSR1, on_usr1);
	signal(SIGTERM, on_term);
	signal(SIGINT, on_term);
	unlink(DONE_FILE);
	int fd = open("/dev/dri/card0", O_RDWR | O_CLOEXEC);
	if (fd < 0 || drm_setup(fd)) {
		fprintf(stderr, "nspanel-splash: no display\n");
		return 1;
	}
	// Only consistent records are played; a broken waiting part means no loop.
	for (unsigned i = 0; i < got; i++)
		if (spans(&recs[i], 0)) {
			got = i;
			break;
		}
	if (got < total) {
		if (got < n_seq)
			n_seq = got;
		n_in = n_loop = n_out = 0;
	}
	const struct rec *seq = recs, *fade_in = seq + n_seq, *loop = fade_in + n_in, *fade_out = loop + n_loop;

	tick = now();
	play(seq, n_seq);
	// The finished picture stays at least MIN_HOLD, then the light band runs
	// until the app has asked and Wi-Fi is up (or CAP has passed).
	double ready = now() + MIN_HOLD;
	while (!quit && now() < ready)
		usleep(20000);
	int waited = 0, wifi = 0;
	while (!quit && now() - t0 < GIVE_UP) {
		wifi = wifi_up();
		if (handoff && (wifi || now() - t0 >= CAP))
			break;
		if (!n_loop) {
			usleep(20000);
			continue;
		}
		if (!waited) {
			tick = now();
			play(fade_in, n_in);
			waited = 1;
		}
		play(loop, n_loop);
	}
	if (waited)
		play(fade_out, n_out);
	if (handoff && !quit) {
		klog(wifi ? "display handed over, Wi-Fi up," : "display handed over without Wi-Fi", now() - t0);
		ioctl(fd, DRM_IOCTL_DROP_MASTER, 0);
		int d = open(DONE_FILE, O_WRONLY | O_CREAT | O_TRUNC, 0644);
		if (d >= 0)
			close(d);
		// Keep the picture (and this buffer) until the app has drawn its own.
		for (int i = 0; i < 300 && !quit; i++)
			usleep(50000);
	}
	unlink(DONE_FILE);
	return 0;
}
