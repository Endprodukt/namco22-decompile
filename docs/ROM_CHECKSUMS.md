# ROM checksums

The games run from the **chip files** inside MAME's zips, not from the zips themselves. A zip's own checksum changes with how
it was packed, so compare the chip files: if yours match the lists below, you are running the same ROMs as the authors.

Each list is also a file in `md5sum` format, so on Linux you can check a whole game at once. Unpack first (the games do this
themselves the first time they start; the folder is named under each game), then, for example:

```bash
cd dirtdash/extracted && md5sum -c ../../docs/roms/dirtdash.md5
```

On Windows, in PowerShell, inside the folder that holds the chips:

```powershell
Get-FileHash -Algorithm MD5 *
```

(or `certutil -hashfile <file> MD5` for one file), and compare with the table. The **CRC32** column is the one MAME's own
ROM lists show.

Rave Racer's `c74.bin` (the sound chip's BIOS) comes from `namcoc74.zip`. The DSP's BIOS, `c71.bin` (MAME's `namcoc71`), is built into all four games and
is not needed; if you have it, its MD5 is `223914888d9be9ffe07952d9aa4c4107` (CRC32 `47c623ab`).

## Prop Cycle

MAME set: `propcycl` (World, PR2 Ver.A). Unpacked chips end up in `extracted/`. 28 files.

| File | Size | MD5 | CRC32 |
|---|---:|---|---|
| `pr1ccrh.1d` | 524288 | `a857f962142f0fd127a825545e5ad318` | `1d68bc31` |
| `pr1ccrl.3d` | 2097152 | `df470abb93c504bc0bebf375e5c2610b` | `e01321fd` |
| `pr1cg0.12b` | 2097152 | `0ed6948004a1908bde046675719a08b7` | `0a041238` |
| `pr1cg1.10d` | 2097152 | `78363c16ea46a3eab94451f7f29762c4` | `7d09e6a7` |
| `pr1cg2.12d` | 2097152 | `98370bc7007db8632608da516d05227a` | `659f006e` |
| `pr1cg3.13d` | 2097152 | `df74b926df4e1330e02cae736dfc5cba` | `d30bffa3` |
| `pr1cg4.14d` | 2097152 | `635db3e8823bfcd3bcc62c2169f6104b` | `f4636cc9` |
| `pr1cg5.16d` | 2097152 | `b2de0fe3f1807559b794dd2ce9076c96` | `97d333de` |
| `pr1cg6.18a` | 2097152 | `6a1234afe531f677fc01a63433d0a792` | `3e081c03` |
| `pr1cg7.15a` | 2097152 | `d0ef791af5d899a7ac14c105a5a5c6d9` | `ec9fc5c8` |
| `pr1data.8k` | 524288 | `035621374f8d88d47bd7d210bec7f3f5` | `2e5767a4` |
| `pr1ptrl0.18k` | 524288 | `2e9a7ee6659279502ed20f6e7b2efdae` | `fddb27a2` |
| `pr1ptrl1.16k` | 524288 | `8ac95ad3ba1de14a040873ce8f187de5` | `6964dd06` |
| `pr1ptrl2.15k` | 524288 | `caab9b4f3a21df6b2eb6f520de956c63` | `4d7ed1d4` |
| `pr1ptrm0.18j` | 524288 | `ee2e617f7ce4b6e0caf7e6cc361397b0` | `b6f204b7` |
| `pr1ptrm1.16j` | 524288 | `d508b9d66fc33a6bf908de63aea0a99b` | `949588b7` |
| `pr1ptrm2.15j` | 524288 | `1d2ef206c495ec1cab0ae1635d6b6eef` | `dc1cef0a` |
| `pr1ptru0.18f` | 524288 | `226806ad705df011044b7b1693d82756` | `5d66a7c4` |
| `pr1ptru1.16f` | 524288 | `07f08ac88693da3ca3bcd9c83e78c10f` | `e9a3f72b` |
| `pr1ptru2.15f` | 524288 | `cbec24ec682982fca74559633a7ef934` | `c346a842` |
| `pr1scg0.12f` | 2097152 | `7b45b300da60d0d2c70852595876e6d7` | `2d09a869` |
| `pr1scg1.10f` | 2097152 | `7a3ac71716198743dce8de38feef6474` | `7433c5bd` |
| `pr1wavea.2l` | 4194304 | `b3378e0978e0b9e4cf66a34a0f44aed1` | `320f3913` |
| `pr1waveb.1l` | 4194304 | `4dbfa6fe13e9e40377b0f0857d632e8f` | `d91acb26` |
| `pr2ver-a.1` | 1048576 | `9e7104e844b8d01f4adaca7582421648` | `3f58594c` |
| `pr2ver-a.2` | 1048576 | `1dd8aa60232b837bc56706e135a9c279` | `c0da354a` |
| `pr2ver-a.3` | 1048576 | `56d0d8771375ebb83ef9b5a4af4e5c9c` | `74bf4b74` |
| `pr2ver-a.4` | 1048576 | `8939346b64c361eb24d8c47aee334952` | `cf4d5638` |

## Rave Racer

MAME set: `raverace` (World, RV2 Ver.B) and `namcoc74`. Unpacked chips end up in `raverace/extracted/`. 36 files.

| File | Size | MD5 | CRC32 |
|---|---:|---|---|
| `c74.bin` | 16384 | `c78a2c4d6071a227e6d49bc009ef7600` | `a3dce360` |
| `rr1gam.2d` | 256 | `8554aa5b8d9b2bfac5222d3391de25f0` | `b2161bce` |
| `rr1gam.3d` | 256 | `8554aa5b8d9b2bfac5222d3391de25f0` | `b2161bce` |
| `rr1gam.4d` | 256 | `8554aa5b8d9b2bfac5222d3391de25f0` | `b2161bce` |
| `rv1ccrh.5c` | 524288 | `c643b58bb0ee2a71c96c3906982b4743` | `a741b262` |
| `rv1ccrl.5a` | 2097152 | `efdd77905c59e08744a08e2fed204e26` | `bc634f72` |
| `rv1cg0.1a` | 2097152 | `90b5089dae9fc3e0bda5e9a5db7b6b5b` | `c518f06b` |
| `rv1cg1.1c` | 2097152 | `d39b32a856eb0bc52a5e3ffa8089de3f` | `6628f792` |
| `rv1cg2.1d` | 2097152 | `e632db51ba9ad3cd93d2e52ccab5de05` | `0b707cc5` |
| `rv1cg3.1e` | 2097152 | `2261cb3d7f51378b6d66b65675632d58` | `39b62921` |
| `rv1cg4.1f` | 2097152 | `00c5eed967cc9e4e0ae236ea72e2f81a` | `a9791ea2` |
| `rv1cg5.1j` | 2097152 | `e4a60f7cc6bd1d42c4d9c7ea0efdaee5` | `b2c79ec1` |
| `rv1cg6.1k` | 2097152 | `bfc63298d453fb619eb970707df80f06` | `8cddedc2` |
| `rv1cg7.1n` | 2097152 | `37cd1dd9f787c88c2c5e7a6950cb49bc` | `b39147ca` |
| `rv1data.6r` | 524288 | `1b6940a91f8d9977c4685c567f3d1b17` | `d358ec20` |
| `rv1eeprm.9e` | 8192 | `1680290e11ced23a4be4e63c31adbc27` | `e00dd412` |
| `rv1potl0.5b` | 524288 | `af1d38846deed52063a79ccbb1a99d9d` | `de2ce519` |
| `rv1potl1.4b` | 524288 | `b8605991f19ab385743888ad1f90c11e` | `2215cb5a` |
| `rv1potl2.3b` | 524288 | `5c2916472435748551665464c8ae6855` | `ddb15bf7` |
| `rv1potl3.2b` | 524288 | `5cfd1204d24028b9baafa8795066d449` | `fa9361ca` |
| `rv1potm0.5c` | 524288 | `516783d75aa6cc6e99d11b520f7bddf3` | `3c024f3a` |
| `rv1potm1.4c` | 524288 | `9bd3b922f7c114d47f6d3ee32670eddb` | `b1a32a68` |
| `rv1potm2.3c` | 524288 | `ee4874e0f964e94e67ecc931dfecfada` | `a414fe15` |
| `rv1potm3.2c` | 524288 | `e0c3e33f610f16ad290f995832b1a1ca` | `2953bbb4` |
| `rv1potu0.5d` | 524288 | `56bb59bbeb17654779a35900015e74e0` | `b9eaf3cc` |
| `rv1potu1.4d` | 524288 | `0a929531264dd8fe8ed6c44c46ac9c82` | `a5c55258` |
| `rv1potu2.3d` | 524288 | `036aef350756c6a577941ab30dad996b` | `c18fcb74` |
| `rv1potu3.2d` | 524288 | `48a5ebb8ebb045723fa53512ffbc38ae` | `79735aaa` |
| `rv1wav0.10r` | 1048576 | `6b4c5bdf146bc97a5caaf79e09bdb37e` | `5aef8143` |
| `rv1wav1.10p` | 1048576 | `2133eb12a2ca59bb2265275ec84b1b73` | `9ed9e6b3` |
| `rv1wav2.10n` | 1048576 | `f171a8dea29b5a72bf239a99950be7c6` | `5af9dc83` |
| `rv1wav3.10l` | 1048576 | `5f126c54f8c2fd9187c296800abe7cd3` | `ffb9ad75` |
| `rv2_prgllb.4d` | 524288 | `76498f25cdc70611dc85823b1d349ff4` | `3017cd1e` |
| `rv2_prglmb.2d` | 524288 | `0ef49ad4beb013ea4c613eccb15dfa80` | `894be0c3` |
| `rv2_prgumb.8d` | 524288 | `377153d7ed0a4eec956cfafc6317b3c5` | `6414a800` |
| `rv2_prguub.6d` | 524288 | `c76d5f5731b7e9e7994ac3f28db57db9` | `a9f18714` |

## Tokyo Wars

MAME set: `tokyowar` (World, TW2 Ver.A). Unpacked chips end up in `tokyowar/extracted/`. 33 files.

| File | Size | MD5 | CRC32 |
|---|---:|---|---|
| `tokyowar_defaults.nv` | 8192 | `e84f6a0ad1bc545d6783d89587337515` | `e8bd7d09` |
| `tw1ccrh.1d` | 524288 | `28fc6d58ba0ca37c3cf3727717d71613` | `ad17e693` |
| `tw1ccrl.3d` | 2097152 | `1e3143cd9728b392f786512616e8982f` | `d08f5794` |
| `tw1cg0.8d` | 2097152 | `5ddaf85ace13c11dfb9c2eaa61cd6378` | `98b9b070` |
| `tw1cg1.10d` | 2097152 | `559f69101b21d9086966f4cbb3d20c67` | `f96a723a` |
| `tw1cg2.12d` | 2097152 | `80ad1684ca14c04723240e8a50426025` | `573e9ded` |
| `tw1cg3.13d` | 2097152 | `cdaf6f767cc7700e3c06b9321b5d75b6` | `302d5c74` |
| `tw1cg4.14d` | 2097152 | `5330d8ad7c1906d470fa364f8216a334` | `ab8aa1df` |
| `tw1cg5.16d` | 2097152 | `a7293131b56990b122b692f35493103b` | `5063f3d0` |
| `tw1cg6.18d` | 2097152 | `408d68b902a491669e4724d89af5c418` | `d764027c` |
| `tw1cg7.19d` | 2097152 | `ea304a368b9dd5b7b2635c9d29744c28` | `12da0a43` |
| `tw1data.8k` | 524288 | `101678e4d132ebfa08b2e43383c59aba` | `bd046e4b` |
| `tw1ptrl0.18k` | 524288 | `6a3355abb08b5a9024d32c3aa7b2146f` | `44ac5e86` |
| `tw1ptrl1.16k` | 524288 | `344a3b9f9f35ae933102eddbaf768638` | `3c769860` |
| `tw1ptrl2.15k` | 524288 | `5d55166b8f403c5ab825875be80f000a` | `6e94103c` |
| `tw1ptrl3.14k` | 524288 | `78df1224db126a6b0657a3841b2866c0` | `e3ce5eb2` |
| `tw1ptrm0.18j` | 524288 | `7f78431b1f0198ff03b4f26e0e5ae6ae` | `e170cea2` |
| `tw1ptrm1.16j` | 524288 | `6369cb27b6da7419865362ef592f479c` | `36a32237` |
| `tw1ptrm2.15j` | 524288 | `c04a752b7be9f2be763241f74946ac2d` | `c426c278` |
| `tw1ptrm3.14j` | 524288 | `7bfbf62759d46e1d1d9b96abe8d3a600` | `d9b9a651` |
| `tw1ptru0.18f` | 524288 | `a302bb75aa891831fbfc92d698ea121e` | `62a9e9fb` |
| `tw1ptru1.16f` | 524288 | `0b7eae13aae322e7fcf9c3ca56e04c59` | `2fd36177` |
| `tw1ptru2.15f` | 524288 | `bc06c1a6af142ce6691baabb68951f02` | `ceacb1c9` |
| `tw1ptru3.14f` | 524288 | `5acc757ea3b43de250764cd9583014d4` | `939044c2` |
| `tw1scg0.12f` | 2097152 | `0f4371ce9c1dfffab1c175b5c4ac9217` | `e3ec4daa` |
| `tw1scg1.10f` | 2097152 | `b5b426813304de23f8b6481de67ac774` | `b18a06e9` |
| `tw1scg2.8f` | 2097152 | `e93fc2e058aa736394049b32ecaf12d1` | `36f8c3d8` |
| `tw1scg3.7f` | 2097152 | `375b822d18fbe0e19e984ab6c5dac2b5` | `8e14d013` |
| `tw1wavea.2l` | 4194304 | `58f5c78ae0a8a2a6b6447f5b6c670ca5` | `ebce6366` |
| `tw2ver-a.1` | 1048576 | `85ae48af51e37e6844208e935447a572` | `2b17ca92` |
| `tw2ver-a.2` | 1048576 | `768a1697e89ba3d4c886b452d510cfe1` | `12da84e3` |
| `tw2ver-a.3` | 1048576 | `15460c962deb86216f00c5619d3d7c0b` | `7d42c516` |
| `tw2ver-a.4` | 1048576 | `b12da5b948b6ea7404777e54e221ac9d` | `b904ed16` |

## Dirt Dash

MAME set: `dirtdash` (World, DT2 Ver.A, MAME's `dirtdasha`). Unpacked chips end up in `dirtdash/extracted/`. 26 files.

| File | Size | MD5 | CRC32 |
|---|---:|---|---|
| `dt1ccrh.1d` | 524288 | `5acabd9bead8907cc8b9da4577e37fbf` | `af257064` |
| `dt1ccrl.3d` | 2097152 | `677daaf8a0483c40835d326d46879300` | `e536b313` |
| `dt1cg0.8d` | 2097152 | `65fbe1567cbb9bf2ff5f32fce0116b5b` | `10ab95e0` |
| `dt1cg1.10d` | 2097152 | `858938d8e58a42ab1e033a58710b6dae` | `d9f1ba53` |
| `dt1cg2.12d` | 2097152 | `609a485d2fca9831e697d8f12ea40bab` | `bd8b1e0b` |
| `dt1cg3.13d` | 2097152 | `2a93301bd87c89a20ffb21de20a75851` | `ba960663` |
| `dt1cg4.14d` | 2097152 | `edcc176f3f2332a73eb1250c33366acf` | `424b9652` |
| `dt1cg5.16d` | 2097152 | `9fb68279f181bfb553a25e7ab7aa3a1a` | `29516626` |
| `dt1cg6.18d` | 2097152 | `8af0ebc3be5419aa85be700d0c60f146` | `e6fa7180` |
| `dt1cg7.19d` | 2097152 | `4433c54422673d8f70ab7184fdfb07e5` | `2ca19153` |
| `dt1dataa.8k` | 524288 | `e477767ed49bdab9d6d7deca34a205bb` | `9bcdea21` |
| `dt1ptrl0.18k` | 524288 | `06c6b3710173c07c1ad8f38eebfd9b97` | `4e0cac3a` |
| `dt1ptrl1.16k` | 524288 | `60ecb0415ddb709c8b0ec11cf539a543` | `59ba9dba` |
| `dt1ptrl2.15k` | 524288 | `09c35244cd23a57deea755ec34078ed9` | `cfe80c67` |
| `dt1ptrm0.18j` | 524288 | `e012b770b344038126d2544dcad6fcb2` | `41f34337` |
| `dt1ptrm1.16j` | 524288 | `dd5b1c48ad96cc1568d01cad7b737bfd` | `f620fd41` |
| `dt1ptrm2.15j` | 524288 | `90de422b5cfe0e352fbb2f4961fab5d1` | `71e6714d` |
| `dt1ptru0.18f` | 524288 | `fd6b86daae8939fd64aae399291a6663` | `4909bd7d` |
| `dt1ptru1.16f` | 524288 | `1311f227590a6ceec93a88f1058b2702` | `4a5097df` |
| `dt1ptru2.15f` | 524288 | `a91c9e24f1b356cd7ec3ecbdd8ee0168` | `1171eaf5` |
| `dt1scg0.12f` | 2097152 | `0b1f79c01ba136432cc1a2974cc33acb` | `a09b5760` |
| `dt1scg1.10f` | 2097152 | `86f95a2b68fbb3b08b2d95be56dae961` | `f9ac8111` |
| `dt1wavea.2l` | 4194304 | `0eb373b848bb30069853412fd9d5c309` | `cbd52e40` |
| `dt1waveb.1l` | 4194304 | `1280e17788bfcf83c6e5bf125f37a4a6` | `6b736f94` |
| `dt2vera.1` | 2097152 | `4fbb2b5ead26717087ac638e0d57aee1` | `402a3d73` |
| `dt2vera.2` | 2097152 | `3ca9ef45fb4e0663b2d15040dcd7e9d2` | `66ed140d` |
