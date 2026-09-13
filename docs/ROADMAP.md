# TeleStash Roadmap — Stabilisasi & Polish (dibuat 2026-09-13)

Roadmap pasca-v1.7.1. Disusun dari audit kode berlapis (3 auditor spesialis paralel:
correctness/perf, security, over-engineering — semua temuan terverifikasi dengan
supervisi manual, file:line tercantum) + keputusan user via sesi grilling dua ronde.

## Prinsip

- **Arah**: stabilisasi & polish, bukan fitur baru besar. Empat rilis, dikerjakan
  santai tanpa tenggat.
- **Target**: pemakaian personal dulu, menyiapkan fondasi rilis publik (GitHub
  public release) di akhir.
- **Blacklist permanen** (jangan tawarkan lagi): TMDB, thumbnail ekstraksi MPV,
  whisper subtitle/translate, proxy/VPN/network-optimizer.

## Riwayat singkat

- v1.5.0–v1.6.x: Tier 1–2 lama (watch history SQLite, error UX, analytics, lalu
  serangkaian hotfix thumbnail/TMDB yang berujung penghapusan).
- v1.7.0: sebagian Tier 3 lama (favorites, tags, per-folder sort, duplicate scan).
- v1.7.1: single-instance enforcement; toolchain lokal dihapus — build HANYA via CI.

---

## R1 (v1.7.2) — Transfer Quick Wins + Pemangkasan

**Tujuan**: bug transfer nyata selesai + repo ramping sebelum pekerjaan besar masuk.

Pemangkasan ponytail (batch awal, hasil audit over-engineering, ~1.700 baris + 5 deps):

1. Hapus 4 deps mati: `sysinfo`, `walkdir`, `urlencoding`, `tauri-plugin-os`
   (0 pemakaian di source) — `Cargo.toml:21,55,58,59`.
2. Hapus 90 unused i18n keys di 13 locale (`app/src/i18n/locales/*.json`).
3. Hapus `ThemeToggle.tsx` (0 importer; AuthWizard punya toggle inline sendiri).
4. Buang struct `TransferPolicy` + DI plumbing `State<Arc<TransferPolicy>>` di ±15
   signature — konstanta fixed (retry=0, limit=0, flood=true) lebih baik jadi const
   di `transfer_retry.rs`. Pertahankan `backoff_ms` dan `transfer_retry.rs`.
   `transfer_policy.rs:10-12`
5. Dedup upload path: `api_upload_file` menggandakan loop retry/FLOOD_WAIT/backoff
   dari fs.rs (~255 baris) — panggil helper upload shared. `api_routes.rs:1154-1408`
6. Kolaps triple delegation `initiate_upload` → `cmd_upload_file` →
   `cmd_upload_file_inner`. `fs.rs:1360-1378, 1729-1748`
7. Bersih-bersih kecil: `FilesResponse.files` double-serialize, sparse-fieldset
   `?fields=` tanpa konsumen, `BandwidthManager.can_transfer` (0 caller), enum
   `AuthState` + struct `Drive` (0 referensi), dedup `media_size`/`mime_type_from_media`.

Fix transfer (hasil audit correctness + verifikasi supervisi):

8. **Fix download chunk retry budget = 0** — `TransferPolicy::retry_attempts()`
   hardcoded 0 dipakai langsung sebagai `chunk_retry_budget`; guard
   `if chunk_retry_budget > 0` berarti satu error chunk = download gagal total +
   file parsial dihapus. Beri retry count nyata (mis. 3) + jangan hapus parsial
   saat masih ada retry tersisa. `fs.rs:1182/1229, 2033/2070-2080`
9. **Streaming retry** — error chunk di tengah stream saat ini langsung
   `yield Err` + putus SETELAH Content-Length dikirim (MPV lihat stream terpotong);
   single-file bahkan putus diam-diam. Tambah satu round re-fetch media fresh +
   retry sebelum menyerah (pola re-fetch sudah ada `preview.rs:261-300`).
   `server.rs:376-385` (split), `server.rs:223-227` (single)
10. **Fix queue download macet setelah cancel** — `finally` tidak memicu queue
    effect, item pending diam selamanya. Salin trigger satu baris dari
    `useFileUpload.ts:129`. `useFileDownload.ts:124-126`
11. **Harden mutex poisoning** — 9 titik `Mutex::lock().unwrap()` jalur transfer
    (satu panic → poisoned → semua transfer berikutnya panic). Ganti error
    mapping. `fs.rs:580,594,1195,1327,1330,1353,1506,1521,2048`
12. **Download stall watchdog** — hang tanpa error saat ini menunggu selamanya;
    timeout N detik tanpa chunk = anggap gagal, retry. `fs.rs:1184-1244, 2035-2082`

Testing R1: unit test retry budget + watchdog + queue trigger; `tsc --noEmit`;
CI full (cargo test --locked --lib). User verifikasi runtime: cancel download
saat antrean berisi, streaming seek-heavy di file split.

---

## R2 (v1.8.0) — Dashboard Gesit (flagship)

**Tujuan**: buka folder = instan; UI tetap responsif saat transfer aktif.
Akar "browsing belum gesit" dari audit: kombinasi tanpa-cache, cache-wipe tiap
un-tray, badai re-render, dan badai unduhan metadata.

1. **Tabel `folder_files` SQLite + delta sync** — `cmd_get_files` tiap buka folder
   menarik SELURUH isi channel dari Telegram ulang (iter_messages tanpa limit,
   plus manifest download per pesan). Simpan metadata file per folder di SQLite;
   buka folder = baca lokal instan + delta sync background.
   `fs.rs:2237-2303`; schema `db.rs:57-130`
2. **Stop cache-wipe saat un-tray** — handler `visibilitychange` saat ini
   invalidates SEMUA `['files']` queries tiap keluar tray (user sering minimize) +
   full `cmd_scan_folders`. Dengan cache SQLite: cukup delta sync ringan.
   `useTelegramConnection.ts:82-103`
3. **Search lokal di atas cache** — search >2 karakter masih ke Telegram
   (SearchGlobal, limit 50, no pagination). Pindah ke query SQLite lokal: instan,
   tak terbatas 50. `fs.rs:2359-2395`, `DesktopDashboard.tsx:418-443`
4. **Fix badai re-render saat transfer aktif** — `favoriteIdSet = new Set(...)`
   dibangun ulang tiap render → invalidasi rantai memo → filter 500 file + regex
   series parse + re-sort + 500 DOM card, 4x/detik dipicu event progress 250ms.
   Fix: `useMemo` untuk `favoriteIdSet`/`displayedFiles`, `React.memo(FileCard)`,
   callback stabil di FileExplorer. `DesktopDashboard.tsx:144-146,179-181`,
   `FileExplorer.tsx:173-183,612-624`, `FileCard.tsx:39`
5. **Metadata video: berhenti di header** — tiap FileCard mengunduh 2 MB penuh
   untuk metadata (moov/EBML biasanya puluhan KB) tanpa cache Rust; scroll
   keluar-masuk viewport = unduh ulang. Fix: stop di deteksi moov/EBML + cache
   disk/DB. `video_metadata.rs:84-115`, `useVideoMetadata.ts:5,35`
6. **Startup paralel** — `initStore` sequential padahal enriched-folders dan
   groups independen → `Promise.all`. `useTelegramConnection.ts:37-77`
7. **Cooldown upload adaptif** — cooldown serial 2s per file (marker ponytail)
   hanya saat benar-benar perlu anti-flood, bukan tiap file kecil.
   `useFileUpload.ts:126`
8. **Bulk download tidak menimpa** — `File::create` memotong file lokal bernama
   sama secara senyap; tambah sufiks `(1)` atau prompt. `useFileDownload.ts:143-152`,
   `fs.rs:2026`

Testing R2: unit test delta-sync + penamaan; tsc; CI; verifikasi manual folder
besar (500+ file) terbuka instan, scroll mulus dengan upload aktif.

---

## R3 (v1.8.1) — Code Health: Refactor fs.rs, Lalu Tests

**Tujuan**: hilangkan kelas bug dengan struktur lebih sehat. Urutan (keputusan
user): **refactor dulu, tests menyusul setelah struktur baru stabil** — dengan
caveat: jalur transfer adalah area paling sensitif, tiap modul yang dipecah
dari fs.rs harus lulus CI test lama (transfer_retry, split_upload_resume,
streaming) sebelum lanjut.

1. **Pecah `fs.rs` (2.811 baris, 6 tanggung jawab)** jadi modul: folder listing,
   upload pipeline, split logic, download pipeline, pause/cancel plumbing, search.
   Target: tidak ada file >1.000 baris. Pecah bertahap per-modul dengan CI hijau
   tiap langkah.
2. **Vitest setup + tests prioritas frontend** — 0 test di `app/src/` hari ini
   (semua 4 test file di `app/scripts/`). Prioritas: `useFileUpload` (queue,
   restore, cancel), `useFileDownload`, `useFloodWait`, `errorHumanizer`,
   `watchHistory` migration.
3. **Tests `api_routes.rs` (1.869 baris, 0 test)** — REST upload path + endpoints
   kritis, mengikuti struktur post-refactor.
4. **Cleanup sisa P3 correctness**: arsip menelan error network sebagai EOF
   (`archive.rs:158,263` — propagate error), cancel pending upload bocorkan
   temp zip (`useFileUpload.ts:189-192` — panggil `cmd_delete_temp_zip`),
   akuntansi bandwidth reserve/release konsisten antara fs.rs dan api_routes
   (`fs.rs:1726` vs `api_routes.rs:1392`).

Testing R3: semua test baru hijau di CI; tsc; tidak ada perubahan perilaku
(perubahan murni struktur + test).

---

## R4 (v1.9.0) — Public-Readiness: Security Sprint + Docs

**Tujuan**: gate rilis publik (GitHub release page). Satu sprint security penuh
(keputusan user) berdasarkan audit keamanan, plus docs akurat.

Security (semua temuan audit security, P1 dulu):

1. **IPC path lockdown (P1)** — commands ber-path menerima path mentah dari
   webview: `cmd_download_file` menulis file ke path apa pun (`fs.rs:1955`),
   `cmd_open_file_externally` eksekusi via ShellExecute tanpa allowlist
   (`lib.rs:185`), `cmd_list_directory_files` enumerate direktori arbitrer
   (`subtitles.rs:356`). Terapkan pattern `cmd_delete_temp_zip` (canonicalize +
   confine ke root hasil dialog) ke semua command ber-path; allowlist extension
   untuk open-external (pdf/mp4/mkv/zip/jpg/...; hard-deny
   hta/url/bat/cmd/scr/exe/js/vbs/lnk/chm/reg).
2. **Ganti crate `rar` 0.4.0 (P1 zip-slip)** — crate menulis entry RAR tanpa
   sanitasi nama; arsip RAR jahat bisa menulis keluar temp dir saat user cuma
   membuka info arsip (bug diverifikasi langsung dari source crate:
   `fs::File::create(format!("{}/{}", path, file.name))`). Ganti binding
   `unrar`/`unrar-ng` yang path output-nya kita kontrol + sanitasi
   `sanitise_entry_name` seperti jalur ZIP/7z. `archive.rs:298,338`
3. **Rapatkan CSP (P1)** — `script-src` saat ini mengizinkan `unsafe-eval`,
   `blob:`, script dari HTTPS mana pun; frontend tidak pakai satu pun script
   remote (semua npm lokal). Ubah ke `script-src 'self'`; `connect-src` cukup
   `'self' http://localhost:*`. `tauri.conf.json:38`
4. **Rate-limit REST API + CORS** — tanpa rate limit untuk destructive ops
   (`api_bulk_files` delete/move, `api_routes.rs:576-823`); buang `Origin: null`
   dari CORS. `lib.rs:142-156`
5. **Share brute-force lockout** — verify password tanpa counter/delay; tambah
   `failed_login_attempts` + `locked_until` di tabel `shared_links` (N gagal →
   lockout 15 menit). `share_routes.rs:309-356`
6. **PII & secrets hygiene** — masking nomor telepon di log (`auth.rs:247`),
   api_hash dari plaintext store ke DPAPI/`CryptProtectData`
   (`AuthWizard.tsx:100`), pastikan log mencatat URL stream tanpa query token.
7. **CI security gate** — `cargo audit` + `npm audit` wajib di release workflow.
8. **Docs + publik**: rapikan README (fitur terkini), AGENTS.md (hapus menyebut
   "URL uploads" — path itu tidak ada di kode), AI_HANDOFF, RELEASE_RUNBOOK;
   tulis threat model singkat (webview = trusted, REST key = local-process trust,
   konten Telegram = untrusted); aktifkan GitHub Issues; update CHANGELOG sesuai
   runbook rilis.

Testing R4: unit test sanitasi path + allowlist + lockout; tsc; CI full termasuk
audit gate; verifikasi manual: RAR jahat (entry `..\..`) ditolak, path traversal
di IPC ditolak, script remote tidak load.

---

## Catatan pelaksanaan

- Versi mengikuti pola ada: R1 patch (v1.7.2), R2 minor (v1.8.0), R3 patch
  (v1.8.1), R4 minor (v1.9.0). Sesuaikan bila scope bergeser.
- Build/test Rust HANYA via GitHub Actions (toolchain lokal sengaja dihapus —
  lihat AGENTS.md). Frontend: `tsc --noEmit` lokal.
- README wajib update untuk rilis dengan perubahan besar (aturan AGENTS.md).
- Temuan audit lengkap tersimpan di memori sesi audit 2026-09-13 (3 auditor
  spesialis + supervisi manual; termasuk temuan bersih: timer pause tidak
  bocor, BandwidthManager seimbang, SQLite tanpa guard-across-await, updater
  supply-chain tertutup, REST API & share routes live bukan dead code); file
  ini merangkum yang masuk eksekusi.
