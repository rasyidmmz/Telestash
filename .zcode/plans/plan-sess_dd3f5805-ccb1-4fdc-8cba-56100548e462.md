## Tujuan
Mengubah TeleStash dari layout fork bergaya Telegram Drive menjadi **Vault Console**: console vault yang ringkas, efisien, dan punya identitas sendiri. Perubahan dikerjakan tetap di branch `redesign/vault-identity`; tidak merge, push, tag, atau release.

## Arah desain yang dipilih
- **Layout:** rail navigasi ikon sempit + command bar/top navigation; tidak lagi memakai sidebar folder penuh sebagai struktur utama.
- **Konten:** grid/list file tetap tersedia untuk efisiensi, tetapi kartu, header, spacing, grouping, dan toolbar didesain ulang agar tidak terasa seperti file manager Telegram Drive.
- **Bahasa visual:** satu aksen gold Vault, charcoal/warm-neutral yang konsisten, tipografi Outfit yang sudah dibundel, hierarchy yang lebih tegas, dan density yang tetap cocok untuk vault besar.
- **Ikon:** migrasi dari Lucide ke Phosphor React sesuai keputusanmu. Tidak mengubah makna aksi atau kontrak event; hanya mengganti komponen ikon dan weight/stroke secara konsisten.

## Tahap implementasi

### 1. Fondasi desain dan ikon
- Tambahkan dependency Phosphor yang disetujui ke `app/package.json` dan lockfile.
- Buat layer token/utility visual di `app/src/App.css` untuk:
  - rail, command bar, content canvas, panel, divider, selected/hover/focus state;
  - radius dan shadow yang tidak seragam secara generik;
  - focus ring dan reduced-motion fallback;
  - menghapus ketergantungan visual pada warna cyan/blue/indigo/slate yang masih tersisa di komponen.
- Audit dan migrasikan seluruh import `lucide-react` di `app/src` (sekitar 33 file) ke Phosphor dengan mapping semantik yang setara. Gunakan satu weight default untuk UI dan weight yang lebih kuat hanya untuk state aktif/brand.
- Rapikan preset tema agar tema bawaan yang masih bernama/berwarna generik tidak merusak identitas Vault; pertahankan ID tema agar preferensi tersimpan tidak putus.

### 2. Shell utama Vault Console
- Refactor `app/src/components/desktop/DesktopDashboard.tsx` pada area render shell, bukan logika query/hook/operasi file:
  - ganti `<Sidebar>` penuh menjadi `VaultRail`/struktur rail yang menampilkan brand, shortcut utama, status koneksi, queue indicator, settings, dan logout;
  - sediakan panel folder sebagai overlay/drawer yang dibuka dari rail, sehingga folder/group/DnD tetap tersedia tanpa menjadi layout permanen;
  - pertahankan seluruh callback folder/group, drop target, resize/collapse semantics yang masih dibutuhkan, tetapi pindahkan presentasinya ke panel yang lebih ringkas;
  - pertahankan modal, player, queue, keyboard shortcut, search, dan event tray yang sudah ada.
- Refactor `app/src/components/desktop/dashboard/TopBar.tsx` menjadi command bar:
  - search/command field sebagai pusat navigasi;
  - current location dan jumlah item sebagai metadata sekunder, bukan breadcrumb dominan `root / folder`;
  - action groups dipisah menjadi primary actions, view/sort, dan utility actions;
  - selection actions muncul sebagai contextual command strip dengan hierarchy yang jelas.
- Bila lebih terawat, pecah shell baru menjadi komponen terfokus di folder dashboard (rail, folder drawer, command bar), tanpa memindahkan business logic ke komponen visual.

### 3. File workspace dan explorer
- Refactor `app/src/components/desktop/dashboard/FileExplorer.tsx`:
  - ubah container menjadi content canvas dengan max-width/spacing yang terkontrol, bukan area padat yang langsung dimulai dari padding standar;
  - jadikan upload sebagai primary empty/add affordance yang menyatu dengan console, bukan selalu kartu dashed yang terlihat seperti template file manager;
  - desain ulang sort/zoom controls menjadi compact control row;
  - pertahankan virtualisasi grid/list, drag-drop, sorting, search results, season grouping, binge mode, subtitle attach, watch progress, dan semua handler yang ada;
  - seragamkan warna series/watch-progress dari cyan/indigo/slate ke semantic Vault tokens.
- Refactor `FileCard.tsx`:
  - pertahankan thumbnail lazy-load, metadata, selection, preview/download/share/delete, drag/drop dan keyboard accessibility;
  - ubah dari kartu 4:3 generik menjadi kartu console dengan hierarchy metadata yang lebih jelas, selected state yang tegas, action rail yang tidak mengambang secara acak, dan motion yang halus;
  - gunakan Phosphor dan semantic tokens.
- Refactor `FileListItem.tsx`:
  - pertahankan virtualized list dan context menu;
  - ubah tabel klasik `# / name / size / date` menjadi list row yang lebih fleksibel dengan metadata yang tetap mudah dipindai;
  - tambahkan focus/pressed/selected state yang konsisten.
- Refactor `EmptyState.tsx`, `DragDropOverlay.tsx`, `RecentWatchBar.tsx`, dan `FileTypeIcon.tsx` agar empty/loading/drop/recent-watch states terasa bagian dari Vault Console, bukan state bawaan file manager.

### 4. Folder/group surface
- Rework `Sidebar.tsx` dan `SidebarItem.tsx` menjadi folder drawer/panel yang dipanggil dari rail:
  - group tabs tetap mendukung create/edit/delete/reorder/assign;
  - folder tetap mendukung open, rename, delete, visibility, invite, reorder, dan drop;
  - status koneksi, sync, bandwidth, serta logout diposisikan sebagai utility area yang tidak mendominasi workspace;
  - pertahankan persistence `telestash_sidebar_width` hanya bila masih relevan; jika drawer tidak lagi resizable, migrasikan tanpa menghapus data secara destruktif.

### 5. Seluruh modal dan layar pendukung
- Audit dan restyle semua komponen dashboard agar shell baru konsisten: `SettingsModal`, `PreviewModal`, `MediaPlayer`, `PdfViewer`, `ArchiveViewerModal`, `ShareDialog`, `RemoteUploadModal`, `MoveToFolderModal`, rename modals, `LogsModal`, `WatchLogsModal`, `StorageAnalyticsModal`, `UploadQueue`, dan `DownloadQueue`.
- Pertahankan API props dan backend invoke command; fokus pada:
  - modal/panel hierarchy, header/footer, spacing, button hierarchy;
  - satu aksen gold dan semantic success/error colors;
  - Phosphor icons;
  - focus trap/keyboard close/focus ring yang sudah ada atau perlu diperbaiki;
  - menghindari modal berlapis untuk interaksi sederhana jika dapat dilakukan inline tanpa mengubah scope fungsi.
- Restyle `AuthWizard`, `ThemeToggle`, `ErrorBoundary`, dan `UpdateBanner` supaya identitas sebelum login dan state error/update sama dengan Vault Console.

### 6. Validasi perilaku dan visual
- Jalankan `npx tsc --noEmit --pretty false` dari `app/`.
- Jalankan test frontend yang tersedia (`npm test` atau subset paling fokus bila test penuh terhambat environment).
- Jalankan `git diff --check`.
- Audit pencarian untuk memastikan tidak ada import `lucide-react` tersisa, tidak ada class warna lama yang menjadi aksen utama, dan tidak ada placeholder.
- Jalankan aplikasi/dev preview dan lakukan pemeriksaan visual pada state penting: login, vault kosong, vault berisi, grid/list, folder drawer/group DnD, selection contextual actions, search, upload/download queues, player, settings, light/dark theme, dan responsive window widths.
- Perbaiki overflow, focus state, kontras, atau regresi interaksi yang ditemukan.

## Batasan eksplisit
- Tidak mengubah backend Rust, protokol transfer, direct-only networking, atau business logic upload/download.
- Tidak menambahkan platform selain Windows 11.
- Tidak merge branch, membuat commit, push, tag, workflow, atau release tanpa instruksi terpisah.
- Tidak menghapus fungsi folder/group/drag-drop hanya karena layout sidebar dihilangkan; fungsi dipindahkan ke drawer/rail.
