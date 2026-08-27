# Payload layouts

What each command in [the command list](README.md#commands) actually reads or writes,
inside the `payload` bytes carried by the [HID frame](hid-frame.md). This is the level
where the device's own data structures live: the key mapping table, LED colors, the profile
block, and the macro table.

## KeyMatrix (keymap)

`GetKeyMatrix`/`SetKeyMatrix` payload: 126 fixed-position slots, one per physical/logical
key position, each a 4-byte big-endian mapping value — `KEYMATRIX_BUFFER_LEN` = 126 × 4 =
504 bytes total, always the full buffer, never a partial slot range.

```
Slot n (0-125) → bytes [n*4 .. n*4+4), big-endian u32
```

Which layer this buffer belongs to (`normal` / `fn` / `fn2`) is selected by the *request's*
`byte1` field, not by anything inside the payload itself — see
[`hid-frame.md`](hid-frame.md#report-body--the-common-request-layout). Not every slot has a
keycap on the A72; unlisted slots are unnamed matrix cells (see `rk-a72-keymap/src/model.rs`
for the A72's specific slot → key-name table — that mapping is per-device, not part of the
wire protocol).

### Mapping value (the 4 bytes at each slot)

```
Bit    31        24 23        16 15                    0
       ┌───────────┬───────────┬───────────────────────┐
       │ type byte │   para    │        keyCode         │
       └───────────┴───────────┴───────────────────────┘
```

- **type byte** (bits 31-24) — a `KeyMappingType` discriminant: `0` = KeyBoard, `1` = Mouse,
  `2` = Media, `3` = Macro, `4` = Custom, `5` = DpiKey, `6` = ProfileSwitch, `7` =
  SpecialFun, `8` = LightSwitch, `9` = ReportRate, `10` = SnipeKey, `11` = PressGun, `13` =
  FnKey, `15` = LodKey, `16` = Pc, `17` = Touch (undocumented by the vendor's own type enum,
  but confirmed on real hardware as the RK-logo "open website" action). Any other byte
  round-trips as `Unknown(byte)`.
- **para** (bits 23-16) — meaning depends on type. For KeyBoard, this is the modifier
  bitmask (see below). For every non-KeyBoard/Macro/Custom type, `type_byte:para:keyCode`
  together are looked up as one opaque 32-bit key against a label table
  (`key_mapping_table.json`) rather than decoded field-by-field — those types don't have a
  documented internal structure, only a table of known whole values and what each means.
- **keyCode** (bits 15-0) — meaning depends on type. For KeyBoard, a HID keyboard usage
  code. For Macro, packs two sub-fields (see below). For Custom, an opaque numeric code
  passed through as-is.

**KeyBoard para (modifier bitmask)**, active bits OR'd together:

```
bit 0 (0x01) LCtrl    bit 4 (0x10) RCtrl
bit 1 (0x02) LShift   bit 5 (0x20) RShift
bit 2 (0x04) LAlt     bit 6 (0x40) RAlt
bit 3 (0x08) LWin     bit 7 (0x80) RWin
```

**Macro keyCode**, split into two bytes rather than one 16-bit value:

```
Bit   15          8 7           0
      ┌────────────┬─────────────┐
      │ repeatCount│ macro index │
      └────────────┴─────────────┘
```
`macro index` (low byte) indexes into the macro table's own array (see below); `repeatCount`
(high byte) is how many times the device replays the macro per key press.

## LED colors (per-key custom RGB)

`GetLedColors`/`SetLedColors` payload: planar (channel-major, not interleaved), 126 slots
per channel — `LED_COLORS_BUFFER_LEN` = 126 × 3 = 378 bytes:

```
Offset        Content
0    .. 126   R, one byte per slot (same slot numbering as KeyMatrix)
126  .. 252   G, one byte per slot
252  .. 378   B, one byte per slot
```

So slot `n`'s color is `(buffer[n], buffer[126+n], buffer[252+n])`. Not every slot has an
individually addressable LED (some ISO-only and media-key positions don't); writing a color
for one of those is accepted but has no visible effect. A color write only becomes visible
if the device's active profile is in SelfDefine mode — see the profile block below.

## Profile block

`GetProfile`/`SetProfile` payload: a 128-byte settings block (`PROFILE_BUFFER_LEN`) covering
device-wide settings beyond keymap/LED colors. Only one field in it is understood and used
by this repo:

```
Offset  Size  Field                    Notes
9       1     LedModeSelection         0 = a built-in lighting effect is active;
                                        1 = SelfDefine (custom per-key colors from
                                            SetLedColors are shown)
33      1     (unnamed marker)         Confirmed via USB capture: the vendor's own
                                        frontend flips this 0 -> 19 in lockstep with
                                        offset 9's 0 -> 1 whenever entering SelfDefine.
                                        Meaning unknown; this repo writes it verbatim
                                        because the device does, not because it's
                                        understood.
```

Every other byte in the 128-byte block is round-tripped untouched by
`LedColorRepository::enter_self_define` (read the whole block, flip these two bytes, write
the whole block back) — its contents beyond these two offsets are not documented here
because they aren't used or decoded by this repo at all.

## Macro table

The reassembled buffer that the [`GetMacros`/`SetMacros` paging requests](hid-frame.md#the-two-exceptions-getmacrossetmacros-paging-requests)
carry, up to `MACRO_BUFFER_LEN` = 4096 bytes. Two-part structure: a fixed-size header table
of `(offset, length)` pairs, one per macro, followed by the macros' own variable-length data
back-to-back.

```
┌─────────────────────────────┬───────────────────────────────────┐
│  Header table (N × 4 bytes) │  Macro data (variable length)      │
│  one entry per macro        │  one variable-length record/macro  │
└─────────────────────────────┴───────────────────────────────────┘
```

**Header entry** (`MACRO_ACTION_LEN` = 4 bytes each, all little-endian u16 pairs):

```
Offset  Size  Field    Notes
0       2     offset   byte offset (from the start of the whole buffer) where this
                       macro's data record begins
2       2     length   byte length of this macro's data record
```

The number of macros is derived from the header table's own byte length: the first 2 bytes
of the *whole buffer* give `header_table_length`; `header_table_length / 4` is the macro
count. A `header_table_length` of 0 means an empty table (no macros).

**One macro's data record** (variable length — a name, then zero or more actions):

```
Offset          Size         Field    Notes
0               1            nameLen  byte length of the encoded name that follows
1               nameLen      name     UTF-16LE, decoded as a whole sequence (not
                                       code-unit-by-code-unit — a name with a
                                       character outside the Basic Multilingual
                                       Plane, e.g. an emoji, needs its surrogate pair
                                       kept together)
1+nameLen       4 per action actions  zero or more 4-byte actions, back-to-back,
                                       filling the rest of the record
```

**One action** (4 bytes), byte-by-byte:

```
Byte 0, bit 7      edge   0 = key-down, 1 = key-up
Byte 0, bits 6-4   kind   3-bit MacroActionKind: 0=NormalKey, 1=ModifyKey, 2=MouseKey,
                          3=MouseCursorX, 4=MouseCursorY, 5=MouseWheel. Values 6-7 are
                          representable but unassigned — decoding one drops that whole
                          macro rather than panicking, since a real device's GetMacros
                          response can contain one, not just malformed input.
Byte 0, bits 3-0   delay  high 4 bits of a 20-bit delay value (bits 19-16)
Byte 1             delay  middle 8 bits (bits 15-8)
Byte 2             delay  low 8 bits (bits 7-0) — the three delay pieces together form
                          one 20-bit millisecond delay before this action fires
Byte 3             key    meaning depends on kind — a HID keycode for NormalKey, a
                          modifier bit for ModifyKey, or a kind-specific code otherwise
```

Macro *names* live in the data records above; nothing in the header table or an action
carries a name — a slot's KeyMatrix mapping only ever references a macro by its numeric
index into this table (see the Macro type's `keyCode` layout under KeyMatrix above), and
`rk-a72 export-hcl`/`get-keymap` resolve that index back to a name by reading the macro
table separately and matching position.
