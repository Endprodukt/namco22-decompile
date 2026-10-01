#!/usr/bin/env python3
"""Put Time Crisis' ROM files where the game looks for them (extracted/) and build timecris_main.bin, the 4 MB 68EC020 program.

    python3 tools/setup_roms.py /path/to/timecris.zip
    python3 tools/setup_roms.py /path/to/folder-with-the-rom-files

MAME set `timecris` (Time Crisis, World TS2 Ver.B; Namco 1995/96, SUPER System 22, MAME namcos22.cpp `timecris_state`). The zip also carries the
Ver.A program under timecrisa/ -- NOT this game. Every chip is checked by SIZE and CRC32 before anything is written; a chip is found by its MAME
name, else by its CRC (older sets name the program ts2ver-b.N). Files are written under the MAME names. The program is the four
ROM_LOAD32_BYTE chips interleaved (ts2verb.1 -> byte 3, .2 -> 2, .3 -> 1, .4 -> 0 of each long).
Exits 0 when extracted/ is complete.
"""
import os
import sys
import zipfile
import zlib

# (region, load kind, name, offset in its region, size, crc32) -- generated from MAME's ROM_START(timecris)
CHIPS = [
    ("maincpu", "32_BYTE", "ts2verb.1", 0x3, 0x100000, 0x29b377f7),
    ("maincpu", "32_BYTE", "ts2verb.2", 0x2, 0x100000, 0x79512e25),
    ("maincpu", "32_BYTE", "ts2verb.3", 0x1, 0x100000, 0x9f4ced33),
    ("maincpu", "32_BYTE", "ts2verb.4", 0x0, 0x100000, 0x3e0cfb38),
    ("mcu", "", "ts1data.8k", 0x0, 0x80000, 0xe68aa973),
    ("sprite", "", "ts1scg0.12f", 0x0, 0x200000, 0x14a3674d),
    ("sprite", "", "ts1scg1.10f", 0x200000, 0x200000, 0x11791dbf),
    ("sprite", "", "ts1scg2.8f", 0x400000, 0x200000, 0xd630fff9),
    ("sprite", "", "ts1scg3.7f", 0x600000, 0x200000, 0x1a62f015),
    ("sprite", "", "ts1scg4.5f", 0x800000, 0x200000, 0x511b8dd6),
    ("sprite", "", "ts1scg5.3f", 0xa00000, 0x200000, 0x553bb246),
    ("textile", "", "ts1cg0.8d", 0x0, 0x200000, 0xde07b22c),
    ("textile", "", "ts1cg1.10d", 0x200000, 0x200000, 0x992d26f6),
    ("textile", "", "ts1cg2.12d", 0x400000, 0x200000, 0x6273954f),
    ("textile", "", "ts1cg3.13d", 0x600000, 0x200000, 0x38171f24),
    ("textile", "", "ts1cg4.14d", 0x800000, 0x200000, 0x51f09856),
    ("textile", "", "ts1cg5.16d", 0xa00000, 0x200000, 0x4cd9fd79),
    ("textile", "", "ts1cg6.18d", 0xc00000, 0x200000, 0xf17f2ec9),
    ("textilemap", "", "ts1ccrl.3d", 0x0, 0x200000, 0x56cad2df),
    ("textilemap", "", "ts1ccrh.1d", 0x200000, 0x80000, 0xa1cc3741),
    ("pointrom", "", "ts1ptrl0.18k", 0x0, 0x80000, 0xe5f2d275),
    ("pointrom", "", "ts1ptrl1.16k", 0x80000, 0x80000, 0x2bba3800),
    ("pointrom", "", "ts1ptrl2.15k", 0x100000, 0x80000, 0xd4441c08),
    ("pointrom", "", "ts1ptrm0.18j", 0x180000, 0x80000, 0x8aea02ba),
    ("pointrom", "", "ts1ptrm1.16j", 0x200000, 0x80000, 0xbccf19bc),
    ("pointrom", "", "ts1ptrm2.15j", 0x280000, 0x80000, 0x7280be31),
    ("pointrom", "", "ts1ptru0.18f", 0x300000, 0x80000, 0xc30d6332),
    ("pointrom", "", "ts1ptru1.16f", 0x380000, 0x80000, 0x993cde84),
    ("pointrom", "", "ts1ptru2.15f", 0x400000, 0x80000, 0x7cb25c73),
    ("c352", "16_WORD_SWAP", "ts1wavea.2l", 0x0, 0x400000, 0xd1123301),
    ("c352", "", "ts1waveb.1l", 0x800000, 0x200000, 0xbf4d7272),
]

EXTRA = {"c71.bin": 0x2000}                # the DSP BIOS (also built into the engine); from the zip when present
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
DEST = os.path.join(ROOT, "extracted")


def top_level(z):
    return {i.filename.lower(): z.read(i) for i in z.infolist() if not i.is_dir() and "/" not in i.filename.strip("/")}


def collect(args):
    found = {}
    for a in args:
        src = os.path.expanduser(a)
        if not os.path.exists(src):
            raise SystemExit(f"Can't find '{src}'. Check the path.")
        paths = [os.path.join(src, e) for e in sorted(os.listdir(src))] if os.path.isdir(src) else [src]
        for p in paths:
            try:
                if p.lower().endswith(".zip"):
                    with zipfile.ZipFile(p) as z:
                        for k, v in top_level(z).items():
                            found.setdefault(k, v)
                elif os.path.isfile(p):
                    with open(p, "rb") as f:
                        found.setdefault(os.path.basename(p).lower(), f.read())
            except zipfile.BadZipFile:
                raise SystemExit(f"'{p}' is not a zip file (or it is damaged).")
    return found


# for tools/rom_checksums.py (the same names as Dirt Dash's setup script): every file the game needs, with its size, and a reader for a zip
REQUIRED = {name: size for region, kind, name, off, size, crc in CHIPS}


def from_zip(path):
    """name -> bytes for every chip file in the zip (by name, else by CRC), as the game's own unpacker would find it"""
    found = collect([path])
    by_crc = {zlib.crc32(v) & 0xFFFFFFFF: v for v in found.values()}
    out = {}
    for region, kind, name, off, size, crc in CHIPS:
        d = found.get(name.lower())
        if d is None or zlib.crc32(d) & 0xFFFFFFFF != crc:
            d = by_crc.get(crc)
        if d is not None:
            out[name] = d
    return out


def have_complete():
    """True when extracted/ holds every chip with the right size (build.sh --check)"""
    return all(os.path.isfile(os.path.join(DEST, n)) and os.path.getsize(os.path.join(DEST, n)) == sz for n, sz in REQUIRED.items()) \
        and os.path.isfile(os.path.join(ROOT, "timecris_main.bin"))


def main():
    args = sys.argv[1:]
    if args == ["--check"]:
        return 0 if have_complete() else 1
    if not args:
        print(__doc__.strip())
        return 2
    found = collect(args)
    by_crc = {}
    for v in found.values():
        by_crc.setdefault(zlib.crc32(v) & 0xFFFFFFFF, v)
    problems, data = [], {}
    for region, kind, name, off, size, crc in CHIPS:
        d = found.get(name.lower())
        if d is None or zlib.crc32(d) & 0xFFFFFFFF != crc:
            d = by_crc.get(crc) if (d is None or zlib.crc32(d) & 0xFFFFFFFF != crc) else d
        if d is None:
            problems.append(f"  missing (by name or CRC {crc:08x}): {name}")
        elif len(d) != size:
            problems.append(f"  wrong size:     {name} ({len(d)} bytes, expected {size})")
        else:
            data[name] = d
    for name, size in EXTRA.items():
        d = found.get(name)
        if d is not None and len(d) == size:
            data[name] = d
    if problems:
        print("This ROM set can't be used:")
        print("\n".join(problems))
        print("You need MAME's 'timecris' (Time Crisis, World TS2 Ver.B) set.")
        return 1
    os.makedirs(DEST, exist_ok=True)
    for name, d in data.items():
        with open(os.path.join(DEST, name), "wb") as f:
            f.write(d)
    prg = bytearray(0x400000)
    for region, kind, name, off, size, crc in CHIPS:
        if region == "maincpu":
            prg[off::4] = data[name]
    with open(os.path.join(ROOT, "timecris_main.bin"), "wb") as f:
        f.write(prg)
    sp, pc = int.from_bytes(prg[0:4], "big"), int.from_bytes(prg[4:8], "big")
    print(f"ROMs OK: {len(data)} files into extracted/, timecris_main.bin built (reset SSP {sp:#010x} PC {pc:#010x})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
