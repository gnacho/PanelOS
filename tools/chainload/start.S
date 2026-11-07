/* SPDX-License-Identifier: AGPL-3.0-or-later */
/*
 * Entry of the chain loader. The stock U-Boot runs the Android-image kernel in
 * place at 0x02080000 with x0 = its device tree. The blob layout (offsets from
 * this entry, see tools/arm64_chain_pack.py):
 *
 *   0x000000  this code
 *   0x0e0000  stack (64 KiB, grows down from 0x0f0000)
 *   0x100000  the kernel's own device tree
 *   0x140000  work area: the merged device tree handed to the kernel
 *   0x180000  the Linux Image (lands on a 2 MiB boundary)
 */
	.section .text.start, "ax"
	.globl	_start
_start:
	mov	x19, x0			/* device tree from U-Boot */
	adr	x20, _start		/* where we run */
	mov	x0, #0xf0000
	add	sp, x20, x0
	mov	x0, x19
	mov	x1, x20
	bl	chain_main		/* returns the device tree for the kernel */
	mov	x19, x0
	mov	x0, #0x180000
	add	x4, x20, x0
	mov	x0, x19
	mov	x1, xzr
	mov	x2, xzr
	mov	x3, xzr
	br	x4
