// Minimal SRT parse/serialize used by the Indonesian translation pipeline.
// Kept as plain ESM so both Vite and `node --test` can import it directly.

export interface SrtCue {
    index: string;
    /** Raw timing line, e.g. "00:01:02,000 --> 00:01:04,500" */
    timing: string;
    /** Text lines (without the index and timing lines). */
    lines: string[];
}

/**
 * Parse SRT content into cues. Tolerates BOM, blank-line variance, and cues
 * with missing sequence numbers. Anything before the first timing line that
 * cannot be attributed to a cue is ignored.
 */
export function parseSrt(content: string): SrtCue[] {
    const text = content.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
    const blocks = text.split(/\n{2,}/);
    const cues: SrtCue[] = [];
    const timingRe = /(\d{1,2}:\d{2}:\d{2}[,.]\d{1,3})\s*-->\s*(\d{1,2}:\d{2}:\d{2}[,.]\d{1,3})/;

    for (const block of blocks) {
        const lines = block.split('\n').filter((l, i, arr) => !(i === arr.length - 1 && l.trim() === ''));
        if (lines.length === 0) continue;

        const timingIdx = lines.findIndex((l) => timingRe.test(l));
        if (timingIdx === -1) continue;

        const timing = lines[timingIdx].trim();
        const textLines = lines.slice(timingIdx + 1);
        if (textLines.length === 0) continue;

        // Sequence number is the first line only when it sits right before timing
        // and is purely numeric.
        let index = String(cues.length + 1);
        if (timingIdx > 0 && /^\d+$/.test(lines[0].trim())) {
            index = lines[0].trim();
        }

        cues.push({ index, timing, lines: textLines });
    }
    return cues;
}

/** Serialize cues back to standard SRT with sequential numbering. */
export function writeSrt(cues: SrtCue[]): string {
    return (
        cues
            .map((cue, i) => `${i + 1}\n${cue.timing}\n${cue.lines.join('\n')}`)
            .join('\n\n') + '\n'
    );
}
