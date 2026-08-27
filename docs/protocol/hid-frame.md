# HID frame layout

The wire-level shape shared by every command in [the command list](README.md#commands): one
519-byte USB HID Feature Report, prefixed on the wire by a 1-byte Report ID. This document
covers only the envelope; what goes in `payload` for each command is in
[`payloads.md`](payloads.md).

```
Report ID (1 byte, always 9)
└─ Report body (519 bytes)
```

## Report body — the common request layout

Every command except the two `SetMacros`/`GetMacros` paging requests (see below) is built
from this one 519-byte layout (`build_request` in `protocol.rs`):

```
Offset  Size  Field         Notes
0       1     opcode        see the command table in README.md
1       1     byte1         meaning depends on the opcode — e.g. KeyMatrix packs
                             `(table << 2) | layer` here (table is always 0 today)
2       1     cmdVal        meaning depends on the opcode (always 0 today — no
                             multi-board support has been exercised)
3       1     fixed marker  always 1
4       1     (unused)      always 0 for this layout — see GetMacros/SetMacros below,
                             which repurpose this byte
5-6     2     dataLength    little-endian; byte count of the payload that follows
7..     up to 512  payload  present on writes; absent (zero-filled) on reads, where
                             dataLength instead tells the device how many bytes to
                             send back
```

`REPORT_LEN` is 519 bytes total (7-byte header + up to 512 bytes of payload); nothing here
is compressed or varint-encoded — every multi-byte field is fixed-width.

## Response layout

A response comes back as the same 519-byte report shape, always with the Report ID byte
still attached (stripped by `parse_response` before the rest is interpreted):

```
Offset  Size  Field         Notes
0       1     cmdId         echoes the opcode that was requested
1       1     byte1         echoes the request's byte1
2       1     cmdVal        echoes the request's cmdVal
3       1     fixed marker  always 1
4       1     (unused)
5-6     2     dataLength    little-endian; byte count of payload that follows
7..     dataLength  payload
```

## The two exceptions: `GetMacros`/`SetMacros` paging requests

The macro table (up to 4096 bytes) doesn't fit in one 512-byte payload, so both directions
page across it — 8 pages of 512 bytes each (`MACRO_BUFFER_LEN` / `MACRO_PAGE_LEN`). Paging
is purely an application-level concern: each page is still sent as its own ordinary 519-byte
Feature Report, just with two fields repurposed from the common layout above.

**`GetMacros` page request** (`build_macro_get_page_request`) — one request per page, each
answered with an ordinary response carrying that page's 512 bytes:

```
Offset  Size  Field         Notes
0       1     opcode        133 (GetMacros)
1-3     3     (unused)      byte1/cmdVal unused; byte 3 still the fixed marker = 1
4       1     pageIndex     0-7 — GetMacros has no packageNum byte, unlike SetMacros
5-6     2     dataLength    always MACRO_PAGE_LEN (512), little-endian
7..     —     (empty)       this is a read; no payload sent
```

**`SetMacros` page request** (`build_macro_set_page_request`) — one fire-and-forget request
per page, no response read (matching the vendor's own configurator):

```
Offset  Size  Field         Notes
0       1     opcode        5 (SetMacros)
1-2     2     (unused)      byte1/cmdVal unused
3       1     packageNum    total page count for this write (NOT the fixed-marker=1
                             the common layout puts here — SetMacros repurposes it)
4       1     packageIndex  0-based index of this page
5-6     2     dataLength    this page's payload length, little-endian (512 for every
                             page except a shorter final page)
7..     dataLength  payload  this page's slice of the encoded macro table
```

The two layouts diverge specifically at bytes 3-4: the common layout's `fixed marker = 1`
at offset 3 is meaningless to GetMacros/SetMacros, which instead use offset 4 (GetMacros) or
offsets 3-4 (SetMacros) for page bookkeeping. See [`payloads.md`](payloads.md#macro-table)
for what the reassembled 4096-byte buffer these pages carry actually contains.
