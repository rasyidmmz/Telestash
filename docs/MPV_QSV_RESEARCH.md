# Riset: Dukungan QSV / Quick Sync pada mpv Windows (TeleStash sidecar)

Status: riset teknis terverifikasi. Semua klaim primer + **diuji langsung di mesin ini**
(Intel HD Graphics 520 `8086:1916` + NVIDIA GeForce 940M) pada 2026.

---

## Ringkasan eksekutif (baca ini dulu)

1. **`-Dqsv=enabled` TIDAK ADA.** mpv tidak punya opsi meson bernama `qsv` sama sekali.
   Flag meson yang disebut di laporan awal (`-Dd3d11`, `-Dvulkan`, `-Dffmpeg:vulkan=auto`)
   **bukan penyebab** masalah ini.
2. **mpv tidak punya kode QSV-specific.** Daftar `--hwdec` dibangun **dinamis dari FFmpeg**.
   `qsv` muncul hanya jika FFmpeg yang di-link punya decoder `h264_qsv` dll.
3. **Akar masalahnya di sisi FFmpeg:** QSV **off secara default** dan butuh `--enable-libvpl`
   (oneVPL) atau `--enable-libmfx` (MediaSDK) saat membangun FFmpeg.
4. **Temuan tak terduga:** binary sidecar TeleStash saat ini **identik secara bit** dengan
   rilis resmi **mpv v0.41.0** (`SHA256` sama persis). Build tersebut memang tidak menyertakan
   libvpl — terbukti dari `ci/build-win32.ps1` milik mpv.
5. **Rekomendasi:** **JANGAN ganti binary.** `d3d11va` sudah tersedia di build saat ini dan
   **berfungsi** di laptop hybrid ini begitu adapter GPU di-pin. TeleStash **sudah**
   mengimplementasikan ini (`HardwareDecodeMode::Adapter`). QSV justru **lebih buruk**:
   di mpv ia **hanya berfungsi sebagai `qsv-copy`** (decode → salin ke RAM → upload ulang).

---

## 1. Opsi distribusi build mpv Windows

### 1a. Distribusi resmi mpv (GitHub Releases) — **TIDAK ada QSV**

- Rilis terbaru: **mpv v0.41.0**, 21 Des 2025 —
  <https://github.com/mpv-player/mpv/releases/tag/v0.41.0>
- Konfigurasi build Windows resminya:
  <https://raw.githubusercontent.com/mpv-player/mpv/master/ci/build-win32.ps1>
  — tidak ada `libvpl`, tidak ada `--enable-libvpl`. Semua dependensi di-vendor lewat
  Meson wrap (`ffmpeg.wrap`, `libplacebo.wrap`, `dav1d.wrap`, …) **tanpa oneVPL**.
- **Diuji di mesin ini** (`mpv-v0.41.0-x86_64-pc-windows-msvc.zip`):
  `mpv --hwdec=help` → **`OFFICIAL: NO QSV`**.

### 1b. shinchiro/mpv-winbuild-cmake — **ADA QSV** (terverifikasi dari sumber)

- `packages/libvpl.cmake` membangun <https://github.com/intel/libvpl> —
  <https://raw.githubusercontent.com/shinchiro/mpv-winbuild-cmake/master/packages/libvpl.cmake>
- `packages/ffmpeg.cmake` memuat `DEPENDS … libvpl` **dan** `--enable-libvpl` —
  <https://raw.githubusercontent.com/shinchiro/mpv-winbuild-cmake/master/packages/ffmpeg.cmake>
- `packages/mpv.cmake` **tidak** memberi flag QSV apa pun (konsisten dengan temuan #2) —
  <https://raw.githubusercontent.com/shinchiro/mpv-winbuild-cmake/master/packages/mpv.cmake>
- README-nya juga mencantumkan `libvpl` di daftar paket —
  <https://github.com/shinchiro/mpv-winbuild-cmake>
- Unduhan: <https://sourceforge.net/projects/mpv-player-windows/files/>

> Status verifikasi: **klaim dari konfigurasi build, bukan dari eksekusi binary.**
> Unduhan SourceForge dari mesin ini hanya mengembalikan halaman HTML (0,13 MB), bukan arsip 7z —
> jadi saya **tidak** bisa menjalankan `--hwdec=help` pada binary shinchiro.
> Karena `--enable-libvpl` + `libvpl` sudah eksplisit ada di konfigurasinya, QSV hampir pasti ada,
> tetapi saya menandainya **belum terverifikasi secara empiris**.

### 1c. zhongfly/mpv-winbuild — **ADA QSV** (terverifikasi empiris di mesin ini) ✅

- Repo: <https://github.com/zhongfly/mpv-winbuild>
- Rilis: <https://github.com/zhongfly/mpv-winbuild/releases>
- README mencantumkan `libvpl` dengan tautan ke <https://github.com/intel/libvpl>
- **Diuji langsung**: `mpv-x86_64-v3-20260923-git-bdefd6cb42.7z` → `mpv --hwdec=help`
  menampilkan **18 baris QSV**, termasuk:
  ```
  qsv (h264_qsv-qsv), qsv (hevc_qsv-qsv), qsv (av1_qsv-qsv), qsv (vp9_qsv-qsv),
  qsv (vvc_qsv-qsv), qsv (mjpeg_qsv-qsv), qsv (mpeg2_qsv-qsv), qsv (vc1_qsv-qsv),
  qsv (vp8_qsv-qsv) + varian qsv-copy untuk semuanya
  ```

### 1d. Cara memverifikasi klaim QSV pada binary mpv mana pun (dipakai di riset ini)

```powershell
mpv.exe --no-config --hwdec=help
# Cari baris "qsv (h264_qsv-qsv)" dan "qsv-copy (h264_qsv-qsv-copy)".
# Tidak ada baris qsv  =>  binary itu dibangun tanpa QSV.
```
Verifikasi tambahan bahwa dekode benar-benar berjalan (bukan sekadar terdaftar):
```powershell
mpv.exe --no-config -v --hwdec=qsv --frames=20 file.mp4
# Cari "[vd] Using hardware decoding (qsv)." atau "(qsv-copy)."
```

---

## 2. Cara mengaktifkan QSV saat build dari sumber

**Kuncinya: FFmpeg, bukan mpv.** mpv tidak punya flag QSV.

### Bukti dari sumber FFmpeg
Dari <https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/configure> (diambil langsung):

```
--enable-libmfx   enable Intel MediaSDK (AKA Quick Sync Video) code via libmfx [no]
--enable-libvpl   enable Intel oneVPL code via libvpl if libmfx is not used [no]
qsv_deps="libmfx"
qsvdec_select="qsv"
h264_qsv_decoder_select="h264_mp4toannexb_bsf qsvdec"
av1_qsv_encoder_deps="libvpl"
```

Poin penting:
- Keduanya **default `[no]`** — **tidak ada auto-deteksi**. Kalau tidak diminta eksplisit,
  `h264_qsv` dan kawan-kawan tidak akan dibangun.
- `qsv_deps="libmfx"` — inti QSV bergantung pada `libmfx`; `--enable-libvpl` adalah jalur
  oneVPL modern ("if libmfx is not used").

### Yang wajib ada
| Kebutuhan | Keterangan |
|---|---|
| `--enable-libvpl` pada FFmpeg | Jalur **modern (oneVPL)**, direkomendasikan 2026 |
| `libvpl` (Intel oneVPL) | <https://github.com/intel/libvpl> — dipakai shinchiro & zhongfly |
| `--enable-libmfx` (alternatif) | Jalur **warisan (MediaSDK)**, sudah usang |
| Driver Intel GPU | Sudah ada di mesin ini (Intel HD 520, `8086:1916`) |

**Tidak diperlukan:**
- ❌ `-Dqsv=enabled` — opsi ini **tidak eksis** di mpv
  (<https://raw.githubusercontent.com/mpv-player/mpv/master/meson.options>)
- ❌ `-Dffmpeg:qsv=enabled` — subproyek FFmpeg di Meson wrap mpv tidak punya opsi `qsv`
- ❌ `-Dlibavdevice` — tidak berhubungan dengan QSV

### Yang tidak berubah
`--enable-libvpl` **hanya** menambah *decoder/encoder* QSV. Ia tidak memberi mpv jalur
interop GPU. Inilah sebabnya di Windows `qsv` (non-copy) **tetap gagal** — lihat §4.

---

## 3. Apakah QSV masih relevan di 2026?

**Masih didukung, tapi bukan jalur yang layak dikejar untuk aplikasi ini.**

- **Masih didukung:** `qsv`/`qsv-copy` masih nilai `--hwdec` yang sah. Maintainer mpv
  (kasper93) menjelaskan mekanisme `qsv-copy` di
  <https://github.com/mpv-player/mpv/discussions/15273>.
  Diuji di mesin ini pada build zhongfly: `Using hardware decoding (qsv-copy).` ✅
- **Tidak ada catatan deprecation** untuk QSV di manual mpv
  (<https://mpv.io/manual/stable/>) maupun di `meson.options`.
- **Intel mengarahkan QSV ke oneVPL** — tercermin dari `--enable-libvpl` yang menggantikan
  `--enable-libmfx` di FFmpeg.
- **Versi mpv relevan:** **v0.41.0** (21 Des 2025), yang **juga** versi sidecar saat ini.
- ⚠️ **Penting:** v0.41.0 mengubah prioritas hwdec — catatan rilis menyebut
  *"Vulkan hardware decoding is preferred over other APIs"* dan
  *"vd_lavc: prefer Vulkan hwdec when available"*. Jadi QSV bukan lagi prioritas utama mpv.

---

## 4. Alternatif yang lebih tahan lama — **ini jawabannya**

Semua hasil di bawah **diukur pada mesin ini** dengan **binary sidecar TeleStash saat ini**
(official mpv v0.41.0), `--vo=gpu-next --gpu-context=d3d11`, video H.264 640×480.

### Matriks hasil (nyata, bukan teori)

| `--hwdec` | adapter default (NVIDIA) | `--d3d11-adapter=Intel` | `--d3d11-adapter=NVIDIA` |
|---|---|---|---|
| `d3d11va` | SOFTWARE ❌ | **HW: d3d11va** ✅ | SOFTWARE ❌ |
| `d3d11va-copy` | SOFTWARE | SOFTWARE | SOFTWARE |
| `vulkan` | SOFTWARE | SOFTWARE | SOFTWARE |
| `vulkan-copy` | SOFTWARE | SOFTWARE | SOFTWARE |
| `auto-safe` | SOFTWARE | **HW: d3d11va** ✅ | SOFTWARE |
| `auto-copy-safe` | SOFTWARE | SOFTWARE | SOFTWARE |

### Temuan krusial
- **Masalah sebenarnya bukan QSV.** Default `--hwdec=auto-safe` **gagal ke software** di
  laptop hybrid ini karena mpv memilih GPU *render* (NVIDIA 940M) dan dekode D3D11VA di
  adapter itu tidak berhasil.
- **Solusinya satu flag:** `--d3d11-adapter=<nama adapter Intel>`. Dengan itu,
  `d3d11va` **berfungsi**. Dokumentasi mpv menyatakan `--d3d11-adapter` memang memengaruhi
  decoder yang memakai helper abstraksi render D3D11 (D3D11VA, DXVA2-DXGI):
  <https://mpv.io/manual/stable/> dan
  <https://raw.githubusercontent.com/mpv-player/mpv/master/DOCS/man/options.rst>
- Daftar adapter tersedia lewat `--d3d11-adapter=help` (di mesin ini: NVIDIA 940M,
  Intel HD 520, Microsoft Basic Render Driver).

### Kenapa QSV kalah untuk kasus ini
- `qsv` (non-copy) **gagal di mesin ini**: `[vd] Could not create device.` — baik dengan
  maupun tanpa `--d3d11-adapter=Intel`. mpv tidak punya driver interop QSV untuk Windows.
- `qsv-copy` **berhasil**, tetapi ia **menyalin frame ke RAM** lalu meng-upload kembali —
  lebih lambat dan lebih boros daya daripada `d3d11va` zero-copy.
- Karena QSV di mpv Windows praktis **hanya copy-mode**, QSV **tidak memberi keuntungan**
  dibanding `d3d11va` + pemilihan adapter.

### Peringkat kematangan untuk mpv di Windows (berdasarkan bukti di atas)
1. **`d3d11va` + `--d3d11-adapter=<iGPU>`** — paling matang, zero-copy, **terbukti jalan** ✅
2. `d3d11va` (tanpa pin adapter) — jalan di banyak mesin, tapi **gagal di hybrid ini** ⚠️
3. `dxva2` — jalur lama, masih ada di build, kurang relevan
4. `vulkan` — diprioritaskan mpv ≥ v0.41.0, tetapi di mesin ini **gagal** (`Could not create device`)
5. `qsv` / `qsv-copy` — hanya copy-mode di Windows; butuh rebuild di sisi FFmpeg
6. **D3D12 Video** — **belum terverifikasi**: saya tidak menemukan opsi/dekoder `d3d12` yang
   dapat dipilih sebagai `--hwdec`. Log mesin ini memang menyebut pixfmt `d3d12` dari decoder,
   tetapi tidak ada hwdec `d3d12` di daftar `--hwdec=help` official maupun zhongfly.

---

## 5. Ukuran binary & penggantian sidecar Tauri

### Ukuran (diukur langsung)

| Binary | Ukuran | QSV |
|---|---|---|
| **Sidecar TeleStash saat ini** (`app/src-tauri/bin/mpv-x86_64-pc-windows-msvc.exe`) | **54,64 MB** | ❌ |
| Official mpv v0.41.0 `mpv.exe` | **54,64 MB** | ❌ |
| zhongfly `mpv.exe` (x86_64-v3) | **117,85 MB** (123.576.320 byte) | ✅ |

**Sidecar TeleStash identik bit-per-bit dengan rilis resmi mpv v0.41.0:**
```
TeleStash sidecar : 6145E63F026451A764077D53FD60860EC9F5C2BC76DCD6E62A88967AC375453D
Official v0.41.0  : 6145E63F026451A764077D53FD60860EC9F5C2BC76DCD6E62A88967AC375453D
```

Mengganti ke build zhongfly berarti **+63,2 MB (+116%)**.

> ⚠️ **Belum terverifikasi:** berapa bagian dari +63 MB itu yang murni QSV.
> Build zhongfly jauh lebih "gemuk" fiturnya (AMF, VAAPI, VapourSynth, codec tambahan).
> Secara teori libvpl statis hanya menambah kisaran 1–3 MB; sisanya fitur lain.
> Saya **tidak** bisa mengisolasi angka QSV-saja dari data yang ada.

### Praktik mengganti sidecar di Tauri v2
Sumber resmi: <https://v2.tauri.app/develop/sidecar/>

- `externalBin` menerima daftar path (relatif/absolut). TeleStash memakai:
  ```json
  "externalBin": ["bin/mpv"]
  ```
- **Binary wajib punya sufiks target-triple.** `bin/mpv` mengharuskan file bernama
  `mpv-$TARGET_TRIPLE.exe` — di TeleStash: `mpv-x86_64-pc-windows-msvc.exe`.
  Ini sudah benar untuk target `x86_64-pc-windows-msvc`.
- Triple host bisa dicek dengan `rustc --print host-tuple`.
- Untuk mengganti: **timpa file** `app/src-tauri/bin/mpv-x86_64-pc-windows-msvc.exe`
  dengan binary baru (nama file **harus** tetap sama), lalu build ulang installer NSIS.
  Tidak ada perubahan `tauri.conf.json` yang diperlukan.
- Izin dijalankan diatur di `src-tauri/capabilities/default.json` lewat
  `shell:allow-execute` dengan `{"name": "bin/mpv", "sidecar": true}` — tidak berubah
  saat binary ditimpa.

---

## 6. Rekomendasi untuk TeleStash

### Rekomendasi utama: **jangan ganti binary**

TeleStash **sudah** mengimplementasikan perbaikan yang benar di
`app/src-tauri/src/commands/streaming.rs`:

```rust
HardwareDecodeMode::Adapter => vec![
    "--hwdec=d3d11va",
    format!("--d3d11-adapter={}", adapter),
],
```

dan sudah punya prober adapter (`--d3d11-adapter=help` → `parse_adapter_list`) plus
validasi di `playback_settings.rs`. Ini **persis** kombinasi yang saya buktikan berfungsi
di mesin hybrid ini.

**Yang perlu dipastikan:**
1. Default `Auto` (`--hwdec=auto-safe`) **gagal ke software** di mesin hybrid ini.
   Pertimbangkan memilih adapter iGPU secara otomatis saat pertama jalan, atau
   mengarahkan pengguna ke mode Adapter ketika `auto-safe` jatuh ke software.
2. Sadari bahwa sejak mpv v0.41.0, `vulkan` diprioritaskan di atas `d3d11va`
   (`auto-safe`); karena `vulkan` **gagal** di mesin ini, eksplisit `--hwdec=d3d11va`
   (seperti yang sudah dilakukan di mode Adapter) adalah pilihan yang tepat.

### Jangan kejar QSV karena:
- Butuh **rebuild FFmpeg dengan `--enable-libvpl`** (bukan flag mpv) — tidak ada
  flag build yang bisa ditambahkan ke sidecar yang sudah jadi.
- Di mpv Windows QSV praktis **hanya `qsv-copy`** (salin ke RAM) → tidak lebih baik
  dari `d3d11va` zero-copy.
- Menambah **+63 MB** ukuran installer untuk keuntungan yang tidak jelas.
- Bahkan build zhongfly yang punya QSV **masih** gagal untuk `qsv` non-copy di mesin ini.

### Kalau tetap ingin QSV
Ganti sidecar dengan build **zhongfly**
(<https://github.com/zhongfly/mpv-winbuild/releases>) — terverifikasi punya QSV,
dan juga memiliki seluruh fitur official **plus** AMF/VAAPI/VapourSynth. Perlu diketahui
ukuran binary naik ke ~118 MB.

---

## 7. Klaim yang BELUM terverifikasi

- Apakah binary shinchiro benar-benar mengekspos `qsv` saat dijalankan — konfigurasinya
  menunjukkan ya (`--enable-libvpl` + `libvpl`), tetapi unduhan SourceForge gagal dari
  mesin ini (hanya mengembalikan HTML), jadi tidak diuji.
- Besaran kontribusi QSV terhadap selisih ukuran +63 MB.
- Dukungan D3D12 Video di mpv: tidak ditemukan `--hwdec=d3d12`; hanya ada pixfmt `d3d12`
  di dalam log decoder.
- Apakah perilaku adapter hybrid ini sama pada kombinasi iGPU/dGPU lain — semua angka
  berasal dari satu mesin (Intel HD 520 + NVIDIA 940M).