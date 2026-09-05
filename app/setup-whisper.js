import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import crypto from 'crypto';
import https from 'https';
import http from 'http';
import { execSync } from 'child_process';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const whisperDir = path.join(__dirname, 'src-tauri', 'resources', 'whisper');

const ZIP_URL = 'https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.1/whisper-bin-x64.zip';
const ZIP_HASH = '7d8be46ecd31828e1eb7a2ecdd0d6b314feafd82163038ab6092594b0a063539';
// Multilingual model: auto-detects the audio language (English, Indonesian, Spanish, ...).
const MODEL_URL = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin';
const MODEL_HASH = '60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe';

function sha256(filePath) {
    const fileBuffer = fs.readFileSync(filePath);
    const hashSum = crypto.createHash('sha256');
    hashSum.update(fileBuffer);
    return hashSum.digest('hex');
}

function fetchWithRedirects(currentUrl, dest, redirectCount = 0) {
    if (redirectCount > 10) {
        return Promise.reject(new Error('Too many redirects while downloading'));
    }

    return new Promise((resolve, reject) => {
        const urlObj = new URL(currentUrl);
        const client = urlObj.protocol === 'http:' ? http : https;

        const options = {
            headers: {
                'User-Agent': 'TeleStash-Build/1.2 (Windows NT 10.0; Win64; x64) Node/' + process.version,
                'Accept': '*/*',
            },
        };

        const req = client.get(currentUrl, options, (response) => {
            if ([301, 302, 303, 307, 308].includes(response.statusCode) && response.headers.location) {
                const redirectedUrl = new URL(response.headers.location, currentUrl).toString();
                response.resume(); // Discard response data
                return resolve(fetchWithRedirects(redirectedUrl, dest, redirectCount + 1));
            }

            if (response.statusCode && (response.statusCode < 200 || response.statusCode >= 300)) {
                response.resume();
                return reject(new Error(`HTTP error ${response.statusCode} for ${currentUrl}`));
            }

            const file = fs.createWriteStream(dest);
            response.pipe(file);

            file.on('finish', () => {
                file.close(resolve);
            });

            file.on('error', (err) => {
                fs.unlink(dest, () => {});
                reject(err);
            });
        });

        req.on('error', (err) => {
            fs.unlink(dest, () => {});
            reject(err);
        });

        req.setTimeout(60000, () => {
            req.destroy(new Error('Download request timed out after 60 seconds'));
        });
    });
}

async function downloadWithRetry(url, dest, maxRetries = 5) {
    let lastError;
    for (let attempt = 1; attempt <= maxRetries; attempt++) {
        try {
            if (attempt > 1) {
                console.log(`Retry attempt ${attempt}/${maxRetries} for ${url}...`);
                await new Promise(r => setTimeout(r, 2000 * attempt));
            }
            await fetchWithRedirects(url, dest);
            return;
        } catch (err) {
            lastError = err;
            console.warn(`Download attempt ${attempt} failed: ${err.message || err}`);
            try { if (fs.existsSync(dest)) fs.unlinkSync(dest); } catch {}
        }
    }
    throw lastError;
}

async function run() {
    fs.mkdirSync(whisperDir, { recursive: true });
    const zipPath = path.join(whisperDir, 'whisper-bin.zip');
    const modelPath = path.join(whisperDir, 'ggml-base.bin');

    console.log('Downloading Whisper CLI...');
    await downloadWithRetry(ZIP_URL, zipPath);
    console.log('Verifying Whisper CLI hash...');
    const zipActualHash = sha256(zipPath);
    if (zipActualHash !== ZIP_HASH) {
        throw new Error(`Whisper ZIP hash mismatch. Expected: ${ZIP_HASH}, Got: ${zipActualHash}`);
    }

    console.log('Downloading Whisper model...');
    await downloadWithRetry(MODEL_URL, modelPath);
    console.log('Verifying Whisper model hash...');
    const modelActualHash = sha256(modelPath);
    if (modelActualHash !== MODEL_HASH) {
        throw new Error(`Whisper Model hash mismatch. Expected: ${MODEL_HASH}, Got: ${modelActualHash}`);
    }

    console.log('Extracting Whisper CLI...');
    execSync(`tar -xf "${zipPath}" -C "${whisperDir}"`);
    fs.unlinkSync(zipPath);

    // Move files from Release/ to whisper/
    const releaseDir = path.join(whisperDir, 'Release');
    if (fs.existsSync(releaseDir)) {
        const files = fs.readdirSync(releaseDir);
        for (const file of files) {
            fs.renameSync(path.join(releaseDir, file), path.join(whisperDir, file));
        }
        fs.rmdirSync(releaseDir);
    }
    console.log('Whisper resources setup successfully.');
}

run().catch(err => {
    console.error(err);
    process.exit(1);
});
