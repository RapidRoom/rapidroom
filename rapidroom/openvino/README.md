# Optional native OpenVINO CPU runtime

The experimental `native-openvino` Cargo feature supports **Linux x86_64** with a locally supplied, pinned ORT 1.24.1 / OpenVINO 2025.4.1 CPU runtime. Stock builds and unsupported platforms retain their existing runtime/provider. No provider, model or telemetry download is added.

The source distribution is `onnxruntime-openvino==1.24.1`. The manifest pins eight original native binaries by size and SHA-256, preserves their `$ORIGIN` dependency layout and uses relative aliases instead of duplicated OpenVINO/frontend/TBB files. The unique native payload is **133,101,397 bytes / 126.935 MiB**, plus notices. Python, GPU/NPU, AUTO/HETERO plugins and weights are absent from the staged runtime. A Python interpreter is needed only to run the build-time staging tool.

The MIT ORT licence and its exact wheel third-party notices are retained alongside the Apache-2.0 OpenVINO licence and runtime/oneTBB/oneDNN notice sets from [OpenVINO source `82bbf0292c5bd490bc25a383e63d6461d3ee05c5`](https://github.com/openvinotoolkit/openvino/tree/82bbf0292c5bd490bc25a383e63d6461d3ee05c5/licensing). The stage tool verifies and copies every notice. It accepts a matching local source directory only; obtaining/redistributing a runtime artifact and enabling the feature in official releases require the maintainer's release decision. The [provider documentation](https://onnxruntime.ai/docs/execution-providers/OpenVINO-ExecutionProvider.html) records this runtime pairing and explicit CPU/thread/stream options.

```sh
python rapidroom/openvino/test_stage.py
python rapidroom/stage-openvino.py --source /local/runtime/capi --check
python rapidroom/stage-openvino.py --source /local/runtime/capi \
  --out src-tauri/resources/openvino-cpu/runtime

# Acquire the shared build lock on a shared machine; use four jobs and no incremental cache.
npm run tauri build -- --no-bundle --features native-openvino -- --locked
```

The output directory must be new. Stage checks fail before publication on missing/tampered binaries, unsupported ELF headers or escaped aliases; a failed copy cleans only its newly created output. Application startup independently verifies the embedded binary/notice checksums and exact aliases, then checks the core C API/version before selecting it. Missing or invalid optional payloads retain the stock ORT CPU path. The optional native core handle lives for the process lifetime; no global library search path is added.

With a selected runtime, only Best/Fast/DeepSharpen sessions explicitly request `OpenVINOExecutionProvider`, CPU, up to four threads and one stream. Session/model initialization failures create an ORT CPU session using the selected core. Each model's actual provider and core build information enter the sensor cache identity, preventing CPU/accelerated or mixed-fallback results from sharing a cache. Original float16 weights, tensor contracts and Sharpen default remain unchanged. This does not claim that every model graph node executes on OpenVINO.

Selecting the optional core also affects the other AI features' runtime, even though their providers are unchanged. Before integration, validate their original checkpoints, cancellation and initialization against the replacement core, plus fresh-process CPU/provider comparisons, metadata/cache/batch saves, developed before/after panels and end-to-end native timings. Prototype low average numerical error does not establish visual equivalence. All normal source/build/hosted gates and local full60 apply; denoised changes need human approval. Nothing here changes the installed daily application or enables an official release bundle automatically.
