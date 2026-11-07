// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Chain loader between the stock U-Boot and the mainline kernel.
//
// The stock U-Boot uses the device tree it hands to the kernel for its own
// display driver too, so it only draws a boot logo with the vendor device
// tree. This loader lets U-Boot have any device tree it likes and gives the
// kernel its own one instead, carrying over what U-Boot fills in at boot:
//
//   - the memory reservations (U-Boot's device tree, the initramfs),
//   - /chosen (command line, initramfs location),
//   - /memory (the DRAM banks, without the firmware's regions),
//   - the serial number.
//
// It also starts the SoC watchdog when the kernel's device tree enables it,
// so a boot that hangs before the kernel feeds the watchdog resets the panel.
//
// Runs with the MMU and data cache off (Linux arm64 boot protocol), hence
// -mstrict-align. Any failure hands over U-Boot's device tree unchanged.

#include <libfdt.h>

#define DTB_OFFSET  0x100000UL
#define WORK_OFFSET 0x140000UL
#define WORK_SIZE   0x40000UL
#define WDT_BASE    0xff1e0000UL

void *chain_main(void *ufdt, char *base);

static int wdt_enabled(const void *fdt)
{
	int off = fdt_node_offset_by_compatible(fdt, -1, "snps,dw-wdt");
	const char *status;

	if (off < 0)
		return 0;
	status = fdt_getprop(fdt, off, "status", NULL);
	return !status || !strcmp(status, "okay") || !strcmp(status, "ok");
}

// DesignWare watchdog: longest timeout, reset on expiry, counter restarted.
static void wdt_start(void)
{
	volatile unsigned int *wdt = (volatile unsigned int *)WDT_BASE;

	wdt[1] = 0xff;                   /* TORR: TOP = TOP_INIT = 15 */
	wdt[0] = (wdt[0] & ~2u) | 1u;    /* CR: reset mode, enable */
	wdt[3] = 0x76;                   /* CRR: restart */
}

static int copy_props(const void *src, int soff, void *dst, int doff, const char *skip)
{
	int prop;

	fdt_for_each_property_offset(prop, src, soff) {
		const char *name;
		int len;
		const void *val = fdt_getprop_by_offset(src, prop, &name, &len);

		if (!val)
			return -1;
		if (skip && !strcmp(name, skip))
			continue;
		if (fdt_setprop(dst, doff, name, val, len))
			return -1;
	}
	return 0;
}

static int node(void *fdt, const char *name)
{
	char path[32] = "/";
	int off;

	strncpy(path + 1, name, sizeof(path) - 2);
	off = fdt_path_offset(fdt, path);
	return off >= 0 ? off : fdt_add_subnode(fdt, 0, name);
}

void *chain_main(void *ufdt, char *base)
{
	void *dtb = base + DTB_OFFSET, *work = base + WORK_OFFSET;
	const void *serial;
	uint64_t addr, size;
	int i, uoff, woff, len;

	if (fdt_check_header(dtb) || fdt_totalsize(dtb) > WORK_SIZE)
		return ufdt;
	if (wdt_enabled(dtb))
		wdt_start();
	if (fdt_check_header(ufdt) || fdt_open_into(dtb, work, WORK_SIZE))
		return ufdt;

	for (i = 0; i < fdt_num_mem_rsv(ufdt); i++)
		if (fdt_get_mem_rsv(ufdt, i, &addr, &size) || fdt_add_mem_rsv(work, addr, size))
			return ufdt;

	uoff = fdt_path_offset(ufdt, "/chosen");
	if (uoff >= 0) {
		woff = node(work, "chosen");
		if (woff < 0 || copy_props(ufdt, uoff, work, woff, "stdout-path"))
			return ufdt;
	}

	uoff = fdt_path_offset(ufdt, "/memory");
	if (uoff < 0)
		return ufdt;
	woff = node(work, "memory");
	if (woff < 0 || copy_props(ufdt, uoff, work, woff, NULL))
		return ufdt;

	serial = fdt_getprop(ufdt, 0, "serial-number", &len);
	if (serial && fdt_setprop(work, 0, "serial-number", serial, len))
		return ufdt;

	fdt_pack(work);
	return work;
}

// The few C library functions libfdt needs; byte-wise, as unaligned word
// accesses fault with the MMU off.
void *memcpy(void *d, const void *s, size_t n)
{
	unsigned char *dp = d;
	const unsigned char *sp = s;

	while (n--)
		*dp++ = *sp++;
	return d;
}

void *memmove(void *d, const void *s, size_t n)
{
	unsigned char *dp = d;
	const unsigned char *sp = s;

	if (dp < sp)
		while (n--)
			*dp++ = *sp++;
	else
		while (n--)
			dp[n] = sp[n];
	return d;
}

void *memset(void *d, int c, size_t n)
{
	unsigned char *dp = d;

	while (n--)
		*dp++ = (unsigned char)c;
	return d;
}

int memcmp(const void *a, const void *b, size_t n)
{
	const unsigned char *x = a, *y = b;

	for (; n; n--, x++, y++)
		if (*x != *y)
			return *x - *y;
	return 0;
}

void *memchr(const void *s, int c, size_t n)
{
	const unsigned char *p = s;

	for (; n; n--, p++)
		if (*p == (unsigned char)c)
			return (void *)p;
	return NULL;
}

size_t strlen(const char *s)
{
	const char *p = s;

	while (*p)
		p++;
	return p - s;
}

size_t strnlen(const char *s, size_t max)
{
	size_t n = 0;

	while (n < max && s[n])
		n++;
	return n;
}

int strcmp(const char *a, const char *b)
{
	while (*a && *a == *b)
		a++, b++;
	return (unsigned char)*a - (unsigned char)*b;
}

char *strncpy(char *d, const char *s, size_t n)
{
	size_t i = 0;

	for (; i < n && s[i]; i++)
		d[i] = s[i];
	for (; i < n; i++)
		d[i] = 0;
	return d;
}

char *strchr(const char *s, int c)
{
	for (; *s; s++)
		if (*s == (char)c)
			return (char *)s;
	return c ? NULL : (char *)s;
}

char *strrchr(const char *s, int c)
{
	const char *last = NULL;

	for (; *s; s++)
		if (*s == (char)c)
			last = s;
	return c ? (char *)last : (char *)s;
}
