// TeleStash Bergamot translate worker.
// Protocol (postMessage):
//   in  { type: 'init', wasmBinary, model, lex, vocab }   -> out { type: 'ready' } | { type: 'error', error }
//   in  { type: 'translate', id, texts: string[] }        -> out { type: 'result', id, results: string[] } | { type: 'error', ... }
//
// The bergamot runtime is an emscripten non-modularized build: a global
// `Module` carrying `wasmBinary` + `onRuntimeInitialized` must exist BEFORE
// importScripts runs. Model/service construction happens inside
// onRuntimeInitialized (fired asynchronously after the wasm instantiates).

var ready = false;
var service = null;
var model = null;

function mkAligned(bytes, alignment) {
    var m = new Module.AlignedMemory(bytes.byteLength, alignment);
    m.getByteArrayView().set(new Uint8Array(bytes));
    return m;
}

function buildEngine(d) {
    try {
        var modelMem = mkAligned(d.model, 256);
        var shortMem = mkAligned(d.lex, 64);
        var vocabMem = mkAligned(d.vocab, 64);
        var vocabs = new Module.AlignedMemoryList();
        vocabs.push_back(vocabMem);
        // "intgemm.alphas" models use the int8shiftAlphaAll GEMM path.
        var config = 'beam-size: 1\nnormalize: 1.0\ngemm-precision: int8shiftAlphaAll\nmax-length-break: 150';
        model = new Module.TranslationModel(config, modelMem, shortMem, vocabs, null);
        service = new Module.BlockingService({ cacheSize: 0 });
        ready = true;
        self.postMessage({ type: 'ready' });
    } catch (err) {
        self.postMessage({ type: 'error', error: 'Engine init failed: ' + String(err) });
    }
}

self.onmessage = function (e) {
    var d = e.data;
    try {
        if (d.type === 'init') {
            self.Module = {
                wasmBinary: d.wasmBinary,
                print: function () {},
                printErr: function () {},
                onRuntimeInitialized: function () {
                    buildEngine(d);
                },
            };
            self.importScripts('/bergamot/bergamot-translator-worker.js');
        } else if (d.type === 'translate') {
            if (!ready || !service || !model) {
                self.postMessage({ type: 'error', id: d.id, error: 'Engine not initialized' });
                return;
            }
            var input = new Module.VectorString();
            d.texts.forEach(function (t) { input.push_back(t); });
            var options = new Module.VectorResponseOptions();
            d.texts.forEach(function () { options.push_back({ qualityScores: false, alignment: false, html: false }); });
            var output = service.translate(model, input, options);
            var results = [];
            for (var i = 0; i < d.texts.length; i++) {
                results.push(output.get(i).getTranslatedText());
            }
            input.delete();
            options.delete();
            output.delete();
            self.postMessage({ type: 'result', id: d.id, results: results });
        }
    } catch (err) {
        self.postMessage({ type: 'error', id: d && d.id, error: String(err) });
    }
};
