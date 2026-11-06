// SPDX-License-Identifier: MIT
/*
 * gpio-pulse: drive one GPIO line low for a while, then high, using the GPIO
 * character device (uAPI v2). Bring-up helper, e.g. to reset the touch
 * controller: gpio-pulse /dev/gpiochip0 12 20 100
 *
 * Usage: gpio-pulse <chip> <line> <low_ms> <wait_after_ms>
 */
#include <fcntl.h>
#include <linux/gpio.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

static void sleep_ms(long ms)
{
	usleep(ms * 1000);
}

int main(int argc, char **argv)
{
	struct gpiochip_info info;
	struct gpio_v2_line_request req;
	struct gpio_v2_line_values vals;
	int fd;

	if (argc != 5) {
		fprintf(stderr, "usage: %s <chip> <line> <low_ms> <wait_after_ms>\n", argv[0]);
		return 2;
	}
	fd = open(argv[1], O_RDWR);
	if (fd < 0) {
		perror(argv[1]);
		return 1;
	}
	if (ioctl(fd, GPIO_GET_CHIPINFO_IOCTL, &info) == 0)
		printf("chip %s: %s (%u lines)\n", argv[1], info.label, info.lines);

	memset(&req, 0, sizeof(req));
	req.offsets[0] = (unsigned int)atoi(argv[2]);
	req.num_lines = 1;
	strncpy(req.consumer, "gpio-pulse", sizeof(req.consumer) - 1);
	req.config.flags = GPIO_V2_LINE_FLAG_OUTPUT;
	req.config.num_attrs = 1;
	req.config.attrs[0].attr.id = GPIO_V2_LINE_ATTR_ID_OUTPUT_VALUES;
	req.config.attrs[0].attr.values = 0;	/* start low */
	req.config.attrs[0].mask = 1;
	if (ioctl(fd, GPIO_V2_GET_LINE_IOCTL, &req) < 0) {
		perror("request line");
		return 1;
	}
	printf("line %s low for %s ms\n", argv[2], argv[3]);
	sleep_ms(atol(argv[3]));

	vals.bits = 1;
	vals.mask = 1;
	if (ioctl(req.fd, GPIO_V2_LINE_SET_VALUES_IOCTL, &vals) < 0) {
		perror("set high");
		return 1;
	}
	printf("line %s high, waiting %s ms\n", argv[2], argv[4]);
	sleep_ms(atol(argv[4]));
	close(req.fd);
	close(fd);
	return 0;
}
