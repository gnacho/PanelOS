// SPDX-License-Identifier: MIT
/*
 * touch-paint: bring-up test for display + touch. Clears /dev/fb0, draws
 * markers in the four corners, then paints a dot wherever the screen is
 * touched (one colour per finger slot). Stops after <seconds> (default 300).
 *
 * Usage: touch-paint [/dev/input/eventN] [seconds]
 */
#include <fcntl.h>
#include <linux/fb.h>
#include <linux/input.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#define MAX_SLOTS 5

static uint8_t *fb;
static struct fb_var_screeninfo var;
static struct fb_fix_screeninfo fix;

static void put(int x, int y, uint32_t c)
{
	if (x < 0 || y < 0 || x >= (int)var.xres || y >= (int)var.yres)
		return;
	*(uint32_t *)(fb + y * fix.line_length + x * 4) = c;
}

static void dot(int cx, int cy, int r, uint32_t c)
{
	for (int y = -r; y <= r; y++)
		for (int x = -r; x <= r; x++)
			if (x * x + y * y <= r * r)
				put(cx + x, cy + y, c);
}

static void clear(uint32_t c)
{
	for (unsigned int y = 0; y < var.yres; y++)
		for (unsigned int x = 0; x < var.xres; x++)
			put(x, y, c);
}

static int find_touch(char *path, size_t len)
{
	for (int i = 0; i < 16; i++) {
		char name[128] = "";
		int fd;

		snprintf(path, len, "/dev/input/event%d", i);
		fd = open(path, O_RDONLY);
		if (fd < 0)
			continue;
		ioctl(fd, EVIOCGNAME(sizeof(name)), name);
		close(fd);
		if (strstr(name, "ouch"))
			return 0;
	}
	return -1;
}

int main(int argc, char **argv)
{
	static const uint32_t colors[MAX_SLOTS] = {
		0xffff4040, 0xff40ff40, 0xff4080ff, 0xffffff40, 0xffff40ff,
	};
	char path[64];
	int slot = 0, x[MAX_SLOTS] = { 0 }, y[MAX_SLOTS] = { 0 }, down[MAX_SLOTS] = { 0 };
	int seconds = argc > 2 ? atoi(argv[2]) : 300;
	time_t end;
	int fbfd, in;

	setvbuf(stdout, NULL, _IOLBF, 0);
	if (argc > 1 && argv[1][0])
		snprintf(path, sizeof(path), "%s", argv[1]);
	else if (find_touch(path, sizeof(path))) {
		fprintf(stderr, "no touchscreen input device found\n");
		return 1;
	}
	in = open(path, O_RDONLY);
	fbfd = open("/dev/fb0", O_RDWR);
	if (in < 0 || fbfd < 0) {
		perror("open");
		return 1;
	}
	ioctl(fbfd, FBIOGET_VSCREENINFO, &var);
	ioctl(fbfd, FBIOGET_FSCREENINFO, &fix);
	if (var.bits_per_pixel != 32) {
		fprintf(stderr, "only 32 bpp supported (got %u)\n", var.bits_per_pixel);
		return 1;
	}
	fb = mmap(NULL, fix.smem_len, PROT_READ | PROT_WRITE, MAP_SHARED, fbfd, 0);
	if (fb == MAP_FAILED) {
		perror("mmap");
		return 1;
	}

	clear(0xff101018);
	dot(0, 0, 24, 0xffffffff);			/* top left: white */
	dot(var.xres - 1, 0, 24, 0xffff0000);		/* top right: red */
	dot(var.xres - 1, var.yres - 1, 24, 0xff00ff00);	/* bottom right: green */
	dot(0, var.yres - 1, 24, 0xff0060ff);		/* bottom left: blue */
	printf("painting from %s for %d s\n", path, seconds);

	end = time(NULL) + seconds;
	while (time(NULL) < end) {
		struct pollfd p = { .fd = in, .events = POLLIN };
		struct input_event ev;

		if (poll(&p, 1, 500) <= 0)
			continue;
		if (read(in, &ev, sizeof(ev)) != sizeof(ev))
			continue;
		if (ev.type == EV_ABS) {
			if (ev.code == ABS_MT_SLOT && ev.value >= 0 && ev.value < MAX_SLOTS)
				slot = ev.value;
			else if (ev.code == ABS_MT_TRACKING_ID)
				down[slot] = ev.value >= 0;
			else if (ev.code == ABS_MT_POSITION_X)
				x[slot] = ev.value;
			else if (ev.code == ABS_MT_POSITION_Y)
				y[slot] = ev.value;
		} else if (ev.type == EV_SYN && ev.code == SYN_REPORT) {
			for (int s = 0; s < MAX_SLOTS; s++)
				if (down[s]) {
					dot(x[s], y[s], 6, colors[s]);
					printf("slot %d: %d,%d\n", s, x[s], y[s]);
				}
			fflush(stdout);
		}
	}
	return 0;
}
