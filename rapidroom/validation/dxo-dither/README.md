# Highlight dither overflow diagnostic

![Synthetic before/after](before-after.png)

This is a synthetic diagnostic, not a photograph or an application screenshot. It shows the output of the exact before/after `LookupTable::dither` implementations on the final entry of `[65407, 65535]`, for every low-bit random seed from 0 through 2047. Samples are rearranged using `(index * 997) % 2048` to make the wrapped dark values visible.

The calculation starts from 65503 and can reach 65567. Before the fix, 1000 of 2048 samples wrap into 0–31 when converted to `u16`. After saturating the conversion, all outputs stay between 65503 and 65535. The random-state update is identical. Displayed pixels are enlarged with nearest-neighbor scaling; the labels describe the original 16-bit values.

The fork's regression tests cover every one of those seeds, representative unsaturated samples, and the random-state update. The real DxO PureRAW 6/Nikon Z6 II sample is tested separately and is not included in this repository.
