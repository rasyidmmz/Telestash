# TeleStash Threat Model (ringkas, R4)

Dokumen ini mencatat siapa yang dipercaya, siapa yang tidak, dan apa yang
sudah dikeraskan di R4. Windows 11 64-bit saja.

## Zona kepercayaan

| Zona | Status | Alasan |
| :--- | :--- | :--- |
| Webview (UI React lokal) | **Trusted** | Kode TBD milik sendiri, dibundel di installer; tidak memuat script remote. |
| REST key (`127.0.0.1`) | **Local-process trust** | Kunci API hanya berlaku untuk proses di mesin yang sama; bukan autentikasi antar-mesin. |
| Konten dari Telegram | **Untrusted** | Nama file, arsip, subtitle, dan metadata bisa dibuat penyerang; semua parser/consumer harus defensif. |
| Nomor HP & API hash | **PII / secret** | Tidak boleh tampil utuh di log; API hash tidak boleh tersimpan plaintext. |

## Yang sudah dikeraskan (R4)

1. **Jalur file (P0)** — download/list/buka-file lewat IPC divalidasi di
   `commands/path_guard.rs`: kanonikalisasi path, direktori sistem diblokir,
   file executable tidak boleh auto-launch (unknown-ext hanya di dalam
   app-cache dir).
2. **Archive viewer dihapus** — crate `rar`/`sevenz-rust2` dan seluruh UI
   browsing isi arsip dibuang. File `.zip`/`.rar`/`.7z` tetap
   upload/download/share normal, tapi isinya tidak bisa dibuka dari dalam
   app (R4 #2).
3. **CSP dirapatkan** — `script-src 'self'`, `Origin: null` dibuang dari CORS
   REST maupun share/stream (`lib.rs`, `server.rs`). Smoke test PDF viewer
   (pdf.js worker) dilakukan manual tiap rilis karena worker butuh
   `worker-src 'self' blob:` (R4 #3).
4. **Rate limit REST (P1)** — `api_rate_limit.rs`: 240 req/60 mnt per IP,
   kelebihan → `429 + Retry-After`. Hanya di server REST; server
   share/stream tidak di-rate-limit agar streaming media besar tidak
   false-positive (R4 #4).
5. **Share brute-force lockout** — `share_routes.rs`: 5x password salah per
   token per 10 menit → pesan "coba lagi dalam N menit"; sukses me-reset
   counter. Counter in-memory per proses (hilang saat restart — diterima
   karena bcrypt ~300 ms per percobaan tetap memperlambat brute force)
   (R4 #5).
6. **PII hygiene** — nomor HP di-mask di log (`auth.rs`); `api_hash` tidak
   lagi disimpan di `config.json` (memory-only, konfigurasi lama dibersihkan
   otomatis); `api_id` non-secret tetap tersimpan untuk prefill (R4 #6).
7. **CI audit gate** — `ci.yml`: `npm audit --audit-level=high` +
   `cargo audit` menggagalkan build bila ada temuan (R4 #7).

## Risiko sisa (diterima sadar)

- Lockout share hilang saat proses restart (lihat #5).
- `api_hash` masih diketik ulang tiap login segar — tanpa DPAPI/keychain
  (tanpa dependensi baru, sesuai AGENTS.md).
- Verifikasi runtime (playback, PDF smoke) mengandalkan smoke test manual
  karena tidak ada toolchain lokal.

## Di luar scope

Android/Linux/macOS/iOS, proxy/VPN/throttling, URL upload — semua
sengaja tidak didukung (lihat AGENTS.md).
