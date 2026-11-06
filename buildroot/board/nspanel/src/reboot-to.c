// SPDX-License-Identifier: MIT
/*
 * reboot-to: reboot with a mode argument, e.g. "recovery" or "loader".
 * The kernel's syscon reboot-mode driver stores the matching magic value, and
 * the stock U-Boot boots the recovery partition (or enters download mode) once.
 * Busybox `reboot` cannot pass such an argument.
 *
 * Usage: reboot-to <mode>
 */
#include <linux/reboot.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(int argc, char **argv)
{
	if (argc != 2) {
		fprintf(stderr, "usage: %s <mode>   (e.g. recovery, loader)\n", argv[0]);
		return 2;
	}
	sync();
	syscall(SYS_reboot, LINUX_REBOOT_MAGIC1, LINUX_REBOOT_MAGIC2,
		LINUX_REBOOT_CMD_RESTART2, argv[1]);
	perror("reboot");
	return 1;
}
