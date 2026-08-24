# External Subtitles Sidecar & Smart Linkage System Design

## 1. Overview
TeleStash users often store video series (`.mkv`, `.mp4`) and external subtitles (`.sub` + `.idx` VobSub, `.srt`, `.ass`, `.ssa`, `.vtt`) in separate local directories (e.g. `sub/` folder). Uploading these subtitles as regular files directly to a Telegram series channel clutters the file list and degrades the user experience.

This specification defines the **External Subtitles Sidecar System** in TeleStash. It allows users to bulk-upload their video files normally, and at any time attach external subtitle folders/files to their series. Subtitles are linked to video messages and hidden from the regular file explorer list, while seamlessly integrating with the embedded MPV player.

---

## 2. Architecture & Data Model

### A. Subtitle Linkage Database Table
A new SQLite table in `app/src-tauri/src/db.rs` manages the link between video messages and subtitle sidecar messages/files:

```sql
CREATE TABLE IF NOT EXISTS video_subtitles (
    id TEXT PRIMARY KEY,
    folder_id INTEGER,
    video_message_id INTEGER NOT NULL,
    subtitle_message_id INTEGER,
    format TEXT NOT NULL,          -- 'vobsub_idx', 'vobsub_sub', 'srt', 'ass', 'ssa', 'vtt'
    language TEXT NOT NULL,        -- 'en', 'id', 'ja', 'und', etc.
    label TEXT,                    -- e.g. 'English (SDH)', 'Indonesian'
    original_filename TEXT NOT NULL,
    is_paired_vobsub INTEGER DEFAULT 0, -- 1 if part of .sub/.idx pair
    created_at INTEGER NOT NULL
);
```

### B. Telegram Message Caption Tagging
When subtitles are uploaded to the Telegram channel, their captions include a machine-readable sidecar signature:
`#telestash_sub:{video_message_id}:{lang}:{format}`

This ensures that even if TeleStash is reinstalled or opened on another device, the sync engine automatically detects and links all subtitle sidecars to their corresponding videos.

---

## 3. Supported Subtitle Formats

1. **VobSub (`.idx` + `.sub` pair)**:
   - Bitmap subtitle format from DVD/Bluray rips.
   - Requires both files stored under the same base name.
   - Handled by passing `--sub-file=<path_to_idx>` to MPV (MPV automatically locates the adjacent `.sub` file).
2. **SubRip (`.srt`)**: Plain UTF-8 text subtitles.
3. **Advanced SubStation Alpha (`.ass` / `.ssa`)**: Formatted and styled anime/movie subtitles.
4. **WebVTT (`.vtt`)**: Web-based text track subtitles.

---

## 4. User Interaction & Workflow

### A. Action: "Attach Subtitles to Series"
- Available via the folder header action bar: **"Attach Subtitles"** (with icon 💬).
- Also available in file context menus: **"Attach Subtitle File..."**.
- User can select:
  - A directory (e.g. `sub/` containing `.idx`, `.sub`, `.srt`, `.ass` files).
  - Multiple subtitle files directly via the native open dialog.

### B. Smart Episode Matching Algorithm
When a subtitle directory or files are selected, TeleStash analyzes filenames and matches them to videos in the current folder:
1. **Exact Stem Match**: `The.Office.S01E01.mkv` ➔ `The.Office.S01E01.idx`
2. **Season/Episode Token Match**: `The.Office.S01E01.1080p.mkv` ➔ `sub/01x01.en.idx` / `sub/S01E01.srt`
3. **Language Code Extraction**: Detects ISO language codes (e.g. `.en.srt`, `.id.ass`, `.eng.idx`, `[Indonesian].srt`).

### C. Matching Confirmation Modal
Shows a summary table of matched pairs before uploading:
- Video Name ➔ Matched Subtitle(s) ➔ Detected Language.
- Allows user to manually adjust or skip pairs if needed.
- Click **"Upload & Link Subtitles"** to start the background queue.

### D. File Explorer UI
- **Zero Clutter**: Messages tagged with `#telestash_sub` are hidden from the primary file list and series episode view.
- **Visual Badges**: Video items with attached subtitles display a distinctive badge: `[CC]` / `[Sub: EN, ID]`.
- **Subtitle Management Popover**: Clicking the subtitle badge reveals attached tracks with options to preview, rename, or delete tracks.

---

## 5. Playback & MPV Integration

When the user plays a video:
1. `cmd_play_in_mpv` checks `video_subtitles` for any linked subtitles for that `(folder_id, message_id)`.
2. TeleStash checks if the subtitle files are already cached in `app_data_dir/streaming/captions/`.
   - If not cached, TeleStash downloads the small subtitle payloads from Telegram to the local cache directory.
3. MPV is invoked with:
   - For `.srt` / `.ass` / `.vtt`: `--sub-file=<path_to_subtitle>`
   - For VobSub (`.idx` + `.sub`): `--sub-file=<path_to_idx>` (with `.sub` present in the same folder)
   - Multiple tracks: multiple `--sub-file` arguments with `--slang=en,id` preference.
4. User can switch subtitle tracks on-the-fly inside MPV using the `c` key or MPV on-screen controller.

---

## 6. Error Handling & Edge Cases

- **Missing `.sub` in VobSub pair**: If a `.idx` is provided without its corresponding `.sub`, the user is notified with a clear error dialog.
- **Cancelled/Interrupted Upload**: Partial subtitle uploads can be resumed or cleaned up automatically.
- **Network/Offline Playback**: If subtitle cache is present, MPV plays offline without contacting Telegram.

---

## 7. Testing Plan
1. **Parser Tests**: Unit test `seriesParser.ts` and `subtitleMatcher.ts` with various naming conventions (`S01E01`, `1x01`, `Superfan S08E24`, mixed `.idx`/`.sub`/`.srt`/`.ass`).
2. **Database Tests**: Test SQLite migrations and subtitle query functions in `db.rs`.
3. **MPV Integration Tests**: Verify `--sub-file` argument generation for single and multi-track subtitles.
