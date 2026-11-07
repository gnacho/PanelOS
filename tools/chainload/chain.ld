/* SPDX-License-Identifier: AGPL-3.0-or-later */
/* Everything in one block from offset 0; the code must stay below 0xe0000. */
ENTRY(_start)
SECTIONS
{
	. = 0;
	.text : {
		*(.text.start)
		*(.text*)
		*(.rodata*)
		*(.data*)
		*(.bss*)
		*(COMMON)
	}
	/DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
	ASSERT(SIZEOF(.text) < 0xe0000, "chain loader too large")
}
