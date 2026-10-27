# Photo stacks

Select photos in the Library and press Ctrl+G, or open **Stacks** in a thumbnail
context menu. Ctrl+Shift+G unstacks every member of the selected stacks. A stack
belongs to one folder. Virtual copies and cloud placeholders must be imported
as local physical photos before stacking.

The numbered badge expands or collapses the stack in both the Library grid
and the editor filmstrip. Expand, right-click a member and choose **Use this
photo as the cover**. The cover is first in the stored order. Within an album, the lowest-position included member represents the stack;
its stored folder cover may be outside that album. If a filter excludes this
representative, the collapsed stack is hidden rather than another member
appearing. Expanded members stay together in stored order. Existing RAW+JPEG pairing continues
for unstacked photos; explicit stack members remain visible when expanded,
regardless of the RAW+JPEG preference.

**Auto-stack by capture time** previews the entire loaded folder or album,
including photos hidden by filters. It groups consecutive captures in each
folder when the gap from the previous exposure's end to the next capture is
at most the chosen number of seconds (default 1), using DateTimeOriginal,
SubSecTimeOriginal and ExposureTime. A RAW+JPEG pair from one capture does not
become a two-photo stack. Existing stacks and photos
without valid capture dates are skipped. When a proposed group's exposure
bias (ExposureBiasValue) differs between members, as in an exposure bracket, the
frame at the median exposure becomes its cover. For a two-frame bracket, the
exposure bias nearest zero leads; bursts and symmetric ties keep capture order. No stack
writes happen until Apply.

Stars and pick/reject flags affect the selected cover by default. Enable
**Apply stars and flags to whole stacks (this session)** in the stack menu to
include every physical member, including members outside an album's displayed
subset. **Stack saved merge results with sources (this session)** makes saved
focus-stack, HDR and panorama results the cover; it starts disabled. A merge
save remains successful if stacking fails, and the app reports that separately.

The **Use saved photo stacks** grouping option in Batch Rename supplies the
`{group}` and `{member}` tokens from persisted stack membership. Unstacked
photos each get their own group. A member number follows its saved position,
so renaming a subset can leave gaps.

Membership is stored as `stack: {id, position}` in each `.rrdata` sidecar.
There are no absolute member paths. Keep sidecars with their photos when
renaming or moving a folder. Older versions ignore this extra field, but
saving a sidecar with an older version can drop it. Card mode refuses all
stack writes. Each sidecar replacement is atomic; a failed batch attempts to
restore every already-written sidecar and reports any rollback failure.
