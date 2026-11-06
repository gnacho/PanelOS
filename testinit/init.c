// SPDX-License-Identifier: MIT
/*
 * Minimal PID 1 for bring-up tests on the NSPanel Pro.
 *
 * Logs what the kernel found (to /dev/kmsg, so it ends up in ramoops),
 * switches the backlight on, draws colour bars on /dev/fb0, waits, then does a
 * warm reboot. The warm reboot keeps RAM, so the stock Android can read this
 * boot's log from /sys/fs/pstore afterwards.
 */
#define _GNU_SOURCE
#include <dirent.h>
#include <fcntl.h>
#include <linux/fb.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/mount.h>
#include <sys/reboot.h>
#include <sys/stat.h>
#include <unistd.h>

#define HOLD_SECONDS 40

static int kmsg = -1;

static void say(const char *fmt, ...)
{
	char buf[768];
	int n = snprintf(buf, sizeof(buf), "<5>nspanel-test: ");
	va_list ap;

	va_start(ap, fmt);
	n += vsnprintf(buf + n, sizeof(buf) - n, fmt, ap);
	va_end(ap);
	if (n >= (int)sizeof(buf))
		n = sizeof(buf) - 1;
	if (kmsg >= 0 && write(kmsg, buf, n) < 0)
		kmsg = -1;
}

static void dump_file(const char *path)
{
	char buf[1024];
	int fd = open(path, O_RDONLY);
	ssize_t n;
	char *line, *save = NULL;

	if (fd < 0) {
		say("%s: (missing)\n", path);
		return;
	}
	n = read(fd, buf, sizeof(buf) - 1);
	close(fd);
	if (n < 0)
		n = 0;
	buf[n] = 0;
	for (line = strtok_r(buf, "\n", &save); line; line = strtok_r(NULL, "\n", &save))
		say("%s: %s\n", path, line);
}

static void list_dir(const char *path)
{
	char out[600] = "";
	DIR *d = opendir(path);
	struct dirent *e;

	if (!d) {
		say("%s/: (missing)\n", path);
		return;
	}
	while ((e = readdir(d))) {
		if (e->d_name[0] == '.')
			continue;
		if (strlen(out) + strlen(e->d_name) + 2 >= sizeof(out))
			break;
		strcat(out, e->d_name);
		strcat(out, " ");
	}
	closedir(d);
	say("%s/: %s\n", path, out);
}

static void write_file(const char *path, const char *value)
{
	int fd = open(path, O_WRONLY);

	if (fd < 0) {
		say("write %s: open failed\n", path);
		return;
	}
	if (write(fd, value, strlen(value)) < 0)
		say("write %s: failed\n", path);
	close(fd);
}

static void drm_status(void)
{
	DIR *d = opendir("/sys/class/drm");
	struct dirent *e;
	char path[300];

	if (!d)
		return;
	while ((e = readdir(d))) {
		if (strncmp(e->d_name, "card", 4) || !strchr(e->d_name, '-'))
			continue;
		snprintf(path, sizeof(path), "/sys/class/drm/%s/status", e->d_name);
		dump_file(path);
		snprintf(path, sizeof(path), "/sys/class/drm/%s/modes", e->d_name);
		dump_file(path);
	}
	closedir(d);
}

static void color_bars(void)
{
	static const uint32_t colors[] = {
		0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffffff,
		0xffffff00, 0xff00ffff, 0xffff00ff, 0xff000000,
	};
	struct fb_var_screeninfo var;
	struct fb_fix_screeninfo fix;
	uint8_t *fb;
	unsigned int x, y;
	int fd = open("/dev/fb0", O_RDWR);

	if (fd < 0) {
		say("/dev/fb0: not available\n");
		return;
	}
	if (ioctl(fd, FBIOGET_VSCREENINFO, &var) || ioctl(fd, FBIOGET_FSCREENINFO, &fix)) {
		say("/dev/fb0: ioctl failed\n");
		close(fd);
		return;
	}
	say("fb0: %ux%u, %u bpp, line %u bytes\n", var.xres, var.yres,
	    var.bits_per_pixel, fix.line_length);
	fb = mmap(NULL, fix.smem_len, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	if (fb == MAP_FAILED) {
		say("fb0: mmap failed\n");
		close(fd);
		return;
	}
	for (y = 0; y < var.yres; y++) {
		uint32_t c = colors[(y * 8) / var.yres];

		for (x = 0; x < var.xres; x++) {
			uint8_t *p = fb + y * fix.line_length + x * (var.bits_per_pixel / 8);

			if (var.bits_per_pixel == 32) {
				*(uint32_t *)p = c;
			} else if (var.bits_per_pixel == 16) {
				uint16_t r = (c >> 19) & 0x1f, g = (c >> 10) & 0x3f, b = (c >> 3) & 0x1f;

				*(uint16_t *)p = (r << 11) | (g << 5) | b;
			}
		}
	}
	msync(fb, fix.smem_len, MS_SYNC);
	munmap(fb, fix.smem_len);
	close(fd);
	say("fb0: colour bars drawn\n");
}

static void inventory(const char *stage)
{
	say("---- inventory (%s) ----\n", stage);
	list_dir("/sys/class/drm");
	drm_status();
	list_dir("/sys/class/backlight");
	list_dir("/sys/class/graphics");
	list_dir("/sys/block");
	list_dir("/sys/class/regulator");
	list_dir("/sys/bus/platform/drivers/rockchip-drm");
	dump_file("/sys/kernel/debug/devices_deferred");
}

int main(void)
{
	mount("devtmpfs", "/dev", "devtmpfs", 0, NULL);
	mount("proc", "/proc", "proc", 0, NULL);
	mount("sysfs", "/sys", "sysfs", 0, NULL);
	mkdir("/sys/kernel/debug", 0755);
	mount("debugfs", "/sys/kernel/debug", "debugfs", 0, NULL);

	kmsg = open("/dev/kmsg", O_WRONLY);
	say("init started\n");
	dump_file("/proc/version");
	dump_file("/proc/cmdline");
	inventory("t=0");

	sleep(3);
	inventory("t=3s");

	write_file("/sys/class/backlight/backlight/brightness", "200");
	dump_file("/sys/class/backlight/backlight/actual_brightness");

	sleep(10);
	color_bars();

	say("holding for %d s, then warm reboot to stock\n", HOLD_SECONDS);
	sleep(HOLD_SECONDS);
	say("rebooting\n");
	sync();
	reboot(RB_AUTOBOOT);
	return 0;
}
