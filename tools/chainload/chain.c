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
// It unpacks an LZ4-packed Image when the boot image carries one (see
// tools/arm64_chain_pack.py --lz4), and it starts the SoC watchdog when the
// kernel's device tree enables it,
// so a boot that hangs before the kernel feeds the watchdog resets the panel.
//
// Runs with the MMU and data cache off (Linux arm64 boot protocol), hence
// -mstrict-align. Any failure hands over U-Boot's device tree unchanged.

#include <libfdt.h>

#define DTB_OFFSET   0x100000UL
#define WORK_OFFSET  0x140000UL
#define WORK_SIZE    0x40000UL
#define IMAGE_OFFSET 0x180000UL
#define WDT_BASE     0xff1e0000UL
#define SZ_2M        0x200000UL

/* LZ4-packed Image (tools/arm64_chain_pack.py --lz4): this header, then one
 * LZ4 frame. Unpacked to the start of the first DRAM bank (2 MiB aligned),
 * below this loader, so a boot image that holds it stays small. */
#define PACKED_MAGIC 0x5a4c4e50u /* "PNLZ" */
struct packed_header {
	uint32_t magic;
	uint32_t reserved;
	uint64_t image_size; /* the kernel's full footprint (BSS included) */
	uint64_t lz4_size;
};

void *chain_main(void *ufdt, char *base, uint64_t *entry);

/* The arm64 parts are left out when the decoder is tested on a PC. */
static uint64_t ticks(void)
{
	uint64_t v = 0;
#ifdef __aarch64__
	__asm__ volatile("isb; mrs %0, cntpct_el0" : "=r"(v));
#endif
	return v;
}

static uint64_t tick_rate(void)
{
	uint64_t v = 1;
#ifdef __aarch64__
	__asm__ volatile("mrs %0, cntfrq_el0" : "=r"(v));
#endif
	return v;
}

static uint32_t le32(const uint8_t *p)
{
	return p[0] | p[1] << 8 | p[2] << 16 | (uint32_t)p[3] << 24;
}

/* One LZ4 block into dst at op (matches may reach back into earlier blocks). */
static long lz4_block(const uint8_t *ip, const uint8_t *iend, uint8_t *dst, size_t op, size_t cap)
{
	while (ip < iend) {
		unsigned token = *ip++;
		size_t len = token >> 4;
		if (len == 15) {
			unsigned b;
			do {
				if (ip >= iend)
					return -1;
				b = *ip++;
				len += b;
			} while (b == 255);
		}
		if (len > (size_t)(iend - ip) || len > cap - op)
			return -1;
		for (size_t i = 0; i < len; i++)
			dst[op++] = *ip++;
		if (ip >= iend)
			break; /* the last sequence has literals only */
		if (iend - ip < 2)
			return -1;
		size_t offset = ip[0] | ip[1] << 8;
		ip += 2;
		if (offset == 0 || offset > op)
			return -1;
		size_t mlen = token & 15;
		if (mlen == 15) {
			unsigned b;
			do {
				if (ip >= iend)
					return -1;
				b = *ip++;
				mlen += b;
			} while (b == 255);
		}
		mlen += 4;
		if (mlen > cap - op)
			return -1;
		for (size_t i = 0; i < mlen; i++, op++)
			dst[op] = dst[op - offset];
	}
	return (long)op;
}

/* An LZ4 frame (lz4 command line format); checksums are not verified. */
static long lz4_frame(const uint8_t *src, size_t len, uint8_t *dst, size_t cap)
{
	const uint8_t *ip = src, *iend = src + len;
	size_t op = 0;

	if (len < 7 || le32(ip) != 0x184D2204u)
		return -1;
	uint8_t flg = ip[4];
	ip += 6; /* magic, FLG, BD */
	if ((flg >> 6) != 1)
		return -1;
	if (flg & 0x08)
		ip += 8; /* content size */
	if (flg & 0x01)
		ip += 4; /* dictionary id */
	ip += 1;     /* header checksum */
	while (iend - ip >= 4) {
		uint32_t size = le32(ip);
		ip += 4;
		if (size == 0)
			return (long)op; /* end mark */
		size_t n = size & 0x7fffffffu;
		if (n > (size_t)(iend - ip))
			return -1;
		if (size & 0x80000000u) {
			if (n > cap - op)
				return -1;
			for (size_t i = 0; i < n; i++)
				dst[op++] = ip[i];
		} else {
			long r = lz4_block(ip, ip + n, dst, op, cap);
			if (r < 0)
				return -1;
			op = (size_t)r;
		}
		ip += n;
		if (flg & 0x10)
			ip += 4; /* block checksum */
	}
	return -1;
}

/* Start of the first DRAM bank in U-Boot's device tree (#address-cells 2). */
static uint64_t first_bank(const void *ufdt)
{
	int off = fdt_path_offset(ufdt, "/memory");
	int len;
	const uint8_t *reg = off >= 0 ? fdt_getprop(ufdt, off, "reg", &len) : NULL;

	if (!reg || len < 16)
		return 0;
	uint64_t v = 0;
	for (int i = 0; i < 8; i++)
		v = v << 8 | reg[i];
	return v;
}

#ifdef __aarch64__
#define PT_OFFSET 0xD0000UL /* level-1 table of the identity map (4 KiB, in the loader area) */

void mmu_off_flush(uint64_t a0, uint64_t e0, uint64_t a1, uint64_t e1, uint64_t a2, uint64_t e2);

static int at_el2(void)
{
	uint64_t el;
	__asm__ volatile("mrs %0, CurrentEL" : "=r"(el));
	return ((el >> 2) & 3) == 2;
}

/* Caches for the unpacking: with the MMU off every access goes to DRAM and
 * unpacking the kernel takes about 7 s. Identity map with 1 GiB blocks:
 * 0-2 GiB (DRAM) normal write-back memory, 2-4 GiB device memory. */
static void mmu_on(uint64_t *table)
{
	for (int i = 0; i < 512; i++)
		table[i] = 0;
	for (uint64_t i = 0; i < 4; i++)
		table[i] = (i << 30) | (1u << 10) | 1u | /* AF, block */
			   (i < 2 ? (3u << 8) : (1u << 2)); /* inner shareable attr 0, or attr 1 */
	uint64_t mair = 0x00ff; /* attr 0: normal write-back, attr 1: device nGnRnE */
	uint64_t tcr = 32 | (1u << 8) | (1u << 10) | (3u << 12) | (1u << 23) | (1u << 31);
	__asm__ volatile(
		"msr mair_el2, %0\n"
		"msr tcr_el2, %1\n"
		"msr ttbr0_el2, %2\n"
		"isb\n"
		"tlbi alle2\n"
		"dsb sy\n"
		"isb\n"
		"mrs x9, sctlr_el2\n"
		"orr x9, x9, #0x1\n"    /* M */
		"orr x9, x9, #0x4\n"    /* C */
		"orr x9, x9, #0x1000\n" /* I */
		"msr sctlr_el2, x9\n"
		"isb\n"
		:: "r"(mair), "r"(tcr), "r"(table) : "x9", "memory");
}
#endif

/* Unpacks a packed Image; the address to jump to, or 0 when there is none
 * or it does not fit below the loader. */
static uint64_t unpack(const void *ufdt, char *base)
{
	const struct packed_header *h = (const struct packed_header *)(base + IMAGE_OFFSET);

	if (h->magic != PACKED_MAGIC)
		return 0;
	uint64_t dest = (first_bank(ufdt) + SZ_2M - 1) & ~(SZ_2M - 1);
	if (!dest || dest + h->image_size > (uint64_t)base)
		return 0;
#ifdef __aarch64__
	int cached = at_el2();
	if (cached)
		mmu_on((uint64_t *)(base + PT_OFFSET));
#endif
	long n = lz4_frame((const uint8_t *)(h + 1), h->lz4_size, (uint8_t *)dest, h->image_size);
#ifdef __aarch64__
	/* Everything written or read while cached goes back to DRAM: the kernel,
	 * the loader (code, page table, stack) and the packed data. */
	if (cached)
		mmu_off_flush(dest, dest + h->image_size, (uint64_t)base, (uint64_t)base + 0xF0000,
			      (uint64_t)h, (uint64_t)(h + 1) + h->lz4_size);
#endif
	if (n < 64 || le32((const uint8_t *)dest + 56) != 0x644d5241u) /* "ARM\x64" */
		return 0;
	return dest;
}

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

void *chain_main(void *ufdt, char *base, uint64_t *entry)
{
	void *dtb = base + DTB_OFFSET, *work = base + WORK_OFFSET;
	const void *serial;
	uint64_t addr, size, start = ticks();
	int i, uoff, woff, len;

	if (fdt_check_header(dtb) || fdt_totalsize(dtb) > WORK_SIZE)
		return ufdt;
	if (wdt_enabled(dtb))
		wdt_start();
	if (fdt_check_header(ufdt))
		return ufdt;
	if (((const struct packed_header *)(base + IMAGE_OFFSET))->magic == PACKED_MAGIC) {
		uint64_t kernel = unpack(ufdt, base);
		if (!kernel)
			for (;;) /* nothing to start: the watchdog resets the panel */
#ifdef __aarch64__
				__asm__ volatile("wfe");
#else
				;
#endif
		*entry = kernel;
	}
	if (fdt_open_into(dtb, work, WORK_SIZE))
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

	/* Time spent here, readable at /proc/device-tree/chosen/panelos,loader-us. */
	woff = fdt_path_offset(work, "/chosen");
	if (woff >= 0)
		fdt_setprop_u32(work, woff, "panelos,loader-us", (uint32_t)((ticks() - start) * 1000000 / tick_rate()));

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
