# Prepare local AI models before travel

RapidRoom can use cached local models without a connection. Prepare them while
you still have internet access, then rehearse with your installed app. This guide
covers Linux; it does not implement the proposed application Offline mode (#261).
Startup release checks can still attempt a connection. Remote assistants, Cloud,
community downloads, and online maps require their own connections.

## Collect a compatible model cache

While connected, use the features you need in RapidRoom so their usual model
downloads finish. Alternatively, obtain a trusted copy of a compatible cache from
another installation. Keep the original filenames and respect each model's
licence. This helper does not obtain or distribute weights.

The included [registry](models/offline-registry.json) lists 13 assets totalling
1,565,912,459 bytes (about 1.46 GiB), including optional Relight normals:

| Feature                           | Files                                                 |
| --------------------------------- | ----------------------------------------------------- |
| Subject selection and local masks | SAM encoder/decoder, U2Net, SkySeg, Depth Anything V2 |
| Keywords                          | CLIP model and tokenizer                              |
| Remove                            | LaMa                                                  |
| Relight normals                   | MoGe                                                  |
| Denoise                           | NIND, Fast, Best, Sharpen                             |

Twelve ONNX checksums match the application pins at the registry's source commit.
The tokenizer checksum records the compatible file used in an installed-app
offline rehearsal; the application itself currently checks only that it exists.
A mismatch needs investigation: do not replace registry hashes merely to make a
check pass. Model updates may require the registry from your app's version.
The registry contains no runtime libraries; install a working RapidRoom build
before leaving.

## Check, then explicitly prepare missing files

From a RapidRoom source checkout, use Python 3.11 or newer. No Python packages are
required. The script and adjacent `models/offline-registry.json` can also be kept
together outside the checkout.

```sh
python3 rapidroom/prepare-offline-models.py --check
```

With no mode argument, the script also checks only. Its default destination is
`$XDG_DATA_HOME/io.github.CyberTimon.RapidRAW/models`, or
`~/.local/share/io.github.CyberTimon.RapidRAW/models` when `XDG_DATA_HOME` is unset.
Use `--cache` to check the cache of the installation/profile you will actually
use. Checking never creates the cache or changes its contents.

To fill missing entries from a local directory, close RapidRoom first, then run:

```sh
python3 rapidroom/prepare-offline-models.py --prepare \
  --source-dir /path/to/trusted-models
python3 rapidroom/prepare-offline-models.py --check
```

Use the same `--cache /path/to/models` on both commands for a custom destination.
`--registry /path/to/registry.json` selects another trusted registry. Preparation
verifies all needed sources before copying, leaves valid existing files intact,
refuses corrupt existing entries and symlinked files/cache directories, and
publishes regular files atomically without replacing existing destinations. It
requires space for missing files plus 1 GiB spare. After a failed or interrupted
copy, successfully published files remain usable; rerun the check and preparation.
An abrupt interruption can leave a `.rapidroom-offline-*` temporary file: remove
only those abandoned temporary files after all preparation processes have exited.

Output is JSON with each file's `verified`, `missing`, or `invalid` status and a
list of files copied by this invocation. Exit status 0 means every registered file
verified, 1 means not ready or an I/O/registry error, and 2 means invalid command
arguments. No network request is made by the helper. It never deletes or repairs
an invalid entry: inspect/back up that file yourself before retrying.

## Rehearse before departure

1. Check the actual destination cache and keep its successful JSON output.
2. Disconnect networking and start a **fresh** instance of the installed app.
3. On a disposable copy of a photo, exercise each feature you need: Sky,
   Foreground, Subject, Depth, Remove, Keywords, optional Relight, and your chosen
   denoise methods. Keywords must be enabled if you plan to use them.
4. Confirm actual results, then export and reopen a result. Check that previews,
   originals, edits and outputs are on local storage available for your trip.
5. If a feature fails, reconnect and resolve it before departure; a successful
   file check alone does not prove runtime compatibility or inference success.

An installed Linux build was rehearsed with these 13 identities on 2026-10-16:
seven GUI model paths plus Fast, Best+Sharpen and NIND CLI runs completed with
outbound networking blocked. That is evidence for this cache/build combination,
not a guarantee for another build, platform, GPU or remote service.
