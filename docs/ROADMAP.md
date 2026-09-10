# TeleStash Roadmap — Tier 1–4 (dibuat 2026-09-06)

Roadmap pasca-v1.4.0. Disusun bersama user lewat sesi brainstorming (superpowers) +
idea-refine (agent-skills) dengan riset Search Cascade Tier-0: telegram.org/faq + blog
resmi, GitHub competitor scan, HN Algolia, dan changelog Jellyfin, dipadukan dengan
inventarisasi kode internal.

## Posisi Strategis (hasil riset 2026-09-06)

- **Tidak ada native Windows Telegram media manager** di liga atas pasar — semua proyek
  Telegram-storage populer adalah downloader/bot/server self-hosted
  (telegram_media_downloader 5.5k★, Telegram-Stremio 606★). Diferensiasi TeleStash:
  native + MPV engine + no-server + privacy.
- Kompetitor media server menyorot **metadata & watch-state** (arah Jellyfin 12.0:
  metadata matching, resume behavior, playlist besar).
- Batasan Telegram: 2 GB free / 4 GB Premium per file; "unlimited storage" tetap
  jualan resmi. **Risiko: akun inactive 18 bulan = auto-delete data** (relevan untuk
  vault jangka panjang → ditemani fitur resilience di Tier 4).
- Pulse HN: skeptisisme "Telegram gratis selamanya?" adalah tema berulang.

## Tier 1 — Foundation & Quick Wins (v1.5.0)

1. ~~**Thumbnail ringan (fallback-only)**~~ — DIHAPUS di v1.6.6. Ekstraksi frame
   via MPV tidak pernah berhasil di mesin user dan, lebih buruk, memicu event
   `stream-playback-started` yang merusak Recent Watch / Next Up. Tidak akan
   ditawarkan kembali.
2. **Watch history → SQLite** — tabel `watch_history`, migrasi dual-read dari
   localStorage (`telestash_recent_watch_v1`), fondasi watch analytics Tier 2.
3. **Error UX** — peta error backend → pesan manusiawi + kode klasifikasi (reuse
   `failure_classifier.rs`), ter-i18n 13 locale; mulai dari 15–20 titik terpanas.
4. **Hygiene** — fix AI_HANDOFF drift, README tray features, dedup CHANGELOG 1.2.x,
   settings baru (batas cache preview, default download folder).

## Tier 2 — Personal Cinema Flagship (v1.6.0)

1. ~~**Metadata TMDB opt-in**~~ — DIHAPUS di v1.6.3 (merumitkan user: wajib API
   key sendiri; user memutuskan tidak jadi). Tabel `file_metadata` didrop,
   command + UI dihapus bersih.
2. **Poster Wall view mode** — tetap ada sebagai card wall 2:3 dengan badges
   metadata yang sama seperti grid (tanpa layanan eksternal, tanpa thumbnail).
3. **Watch Analytics Dashboard** — dari watch history Tier 1: total waktu tonton,
   streak, top series/folder, chart bulanan.

## Tier 3 — Library Power (v1.7.0)

1. **Favorites + manual tags per file** (SQLite), filter chips per folder.
2. **Sort/filter persist per folder** + bulk operations (bulk rename/move/tag).
3. **Collections lintas-folder** — playlist manual level file (bukan group folder).
4. **Duplicate detector enhancement** — deteksi by-name + size (cepat) di samping
   hash size+content yang ada.

## Tier 4 — Operations & Resilience (v1.8.0)

1. **Premium-aware upload** — deteksi `is_premium` (grammers) → threshold split 4 GB
   untuk Premium, 2 GB untuk free (configurable; ubah rule AGENTS.md + Transfer Rules).
2. **FLOOD_WAIT-aware scheduler** — antrean transfer bertahap + prioritas + opsi
   jadwal off-peak + estimasi waktu.
3. **Vault resilience** — guard inactivity 18 bulan (badge last-touch per folder +
   reminder), backup/restore settings+DB, session health check.
4. **Settings expansion** — port stream configurable, cache cleanup schedule,
   dark schedule tema.

## Not Doing (dan alasannya)

- Proxy/VPN/throttle, folder/URL/drag-drop ingest — melanggar AGENTS.md (keputusan
  desain final).
- Whisper/translate subtitle — sudah dihapus bersih di v1.4.0, tidak dikembalikan.
- Metadata TMDB — dihapus di v1.6.3; API key pribadi terlalu rumit untuk user,
  tidak akan ditawarkan kembali.
- Export library & advanced search — ditolak user (09-05).
- Transcoding/HLS/playback webview — sengaja dibuang (v1.2.3); MPV native = moto.
- Multi-akun & sharing publik internet — kompleks + bertentangan dengan
  "personal library boundary".
- Mobile/macOS/Linux — Windows 11 x64 only.

## Asumsi Kunci (validasi saat eksekusi)

- ~~MPV `--vo=image` dapat dipakai ekstraksi thumbnail~~ — TERBUKTI SALAH; fitur
  thumbnail dihapus di v1.6.6.
- grammers 0.10 expose flag `is_premium` (cek saat implementasi Tier 4).
- Migrasi watch history tidak kehilangan data localStorage lama (dual-read).
