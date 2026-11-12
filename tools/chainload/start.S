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
 *   0x180000  the Linux Image (lands on a 2 MiB boundary), or an LZ4-packed
 *             Image that chain_main() unpacks elsewhere
 *
 * chain_main() returns the device tree for the kernel and stores the kernel
 * entry in the slot it gets (preset to the in-place Image).
 */
	.section .text.start, "ax"
	.globl	_start
_start:
	mov	x19, x0			/* device tree from U-Boot */
	adr	x20, _start		/* where we run */
	mov	x0, #0xf0000
	add	sp, x20, x0
	sub	sp, sp, #16
	mov	x0, #0x180000
	add	x0, x20, x0
	str	x0, [sp]		/* default entry: the Image in place */
	mov	x0, x19
	mov	x1, x20
	mov	x2, sp
	bl	chain_main		/* returns the device tree for the kernel */
	mov	x19, x0
	ldr	x4, [sp]
	/* The kernel may have been written as data: fetch it fresh. */
	dsb	sy
	ic	iallu
	dsb	sy
	isb
	mov	x0, x19
	mov	x1, xzr
	mov	x2, xzr
	mov	x3, xzr
	br	x4

/*
 * mmu_off_flush(a0, e0, a1, e1, a2, e2): ends the cached phase of the
 * unpacking (EL2). Data cache off first, so nothing new is cached, then the
 * three address ranges are cleaned and invalidated to the point of coherency,
 * then the MMU goes off. Registers only: the stack is one of the ranges.
 */
	.globl	mmu_off_flush
mmu_off_flush:
	mrs	x9, sctlr_el2
	bic	x9, x9, #0x4		/* C */
	msr	sctlr_el2, x9
	isb
	mrs	x10, ctr_el0
	ubfx	x10, x10, #16, #4
	mov	x11, #4
	lsl	x11, x11, x10		/* data cache line size */
	sub	x12, x11, #1
	bic	x0, x0, x12
1:	cmp	x0, x1
	b.hs	2f
	dc	civac, x0
	add	x0, x0, x11
	b	1b
2:	bic	x2, x2, x12
3:	cmp	x2, x3
	b.hs	4f
	dc	civac, x2
	add	x2, x2, x11
	b	3b
4:	bic	x4, x4, x12
5:	cmp	x4, x5
	b.hs	6f
	dc	civac, x4
	add	x4, x4, x11
	b	5b
6:	dsb	sy
	bic	x9, x9, #0x1		/* M */
	msr	sctlr_el2, x9
	isb
	tlbi	alle2
	dsb	sy
	isb
	ret
