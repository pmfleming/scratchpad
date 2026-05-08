from pathlib import Path

from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.boundsPen import BoundsPen
from fontTools.ttLib import TTFont


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "fonts" / "NotoSansSymbols2-Regular.ttf"
TARGET = ROOT / "fonts" / "ScratchpadControlSymbols-Regular.ttf"

CONTROL_PICTURE_CODEPOINTS = tuple(range(0x2400, 0x2420)) + (0x2421,)

PUA_LABELS = {
    0xF000: "ZWSP",
    0xF001: "ZWNJ",
    0xF002: "ZWJ",
    0xF003: "LRM",
    0xF004: "RLM",
    0xF005: "LRE",
    0xF006: "RLE",
    0xF007: "PDF",
    0xF008: "LRO",
    0xF009: "RLO",
    0xF00A: "WJ",
    0xF00B: "FA",
    0xF00C: "IT",
    0xF00D: "IS",
    0xF00E: "IP",
    0xF00F: "LRI",
    0xF010: "RLI",
    0xF011: "FSI",
    0xF012: "PDI",
    0xF013: "BOM",
    0xF014: "ALM",
    0xF015: "ISS",
    0xF016: "ASS",
    0xF017: "IAFS",
    0xF018: "AAFS",
    0xF019: "NADS",
    0xF01A: "NODS",
}


def rename_font(font: TTFont) -> None:
    names = {
        1: "Scratchpad Control Symbols",
        2: "Regular",
        3: "Scratchpad Control Symbols Regular",
        4: "Scratchpad Control Symbols Regular",
        5: "Version 1.000",
        6: "ScratchpadControlSymbols-Regular",
    }
    for name_id, value in names.items():
        font["name"].setName(value, name_id, 3, 1, 0x409)
        font["name"].setName(value, name_id, 1, 0, 0)


def glyph_for_char(font: TTFont, ch: str) -> str:
    glyph = font.getBestCmap().get(ord(ch))
    if glyph is None:
        raise ValueError(f"source font lacks glyph for {ch!r}")
    return glyph


def glyph_bounds(font: TTFont, glyph_name: str):
    glyph_set = font.getGlyphSet()
    pen = BoundsPen(glyph_set)
    glyph_set[glyph_name].draw(pen)
    return pen.bounds


def build_scaled_component_glyph(
    font: TTFont,
    glyph_name: str,
    *,
    target_width: int = 1000,
    max_ink_width: int = 850,
    max_ink_height: int = 760,
):
    x_min, y_min, x_max, y_max = glyph_bounds(font, glyph_name)
    ink_width = x_max - x_min
    ink_height = y_max - y_min
    scale = min(max_ink_width / ink_width, max_ink_height / ink_height)
    scaled_width = ink_width * scale
    scaled_height = ink_height * scale
    x = (target_width - scaled_width) / 2 - x_min * scale
    y = (max_ink_height - scaled_height) / 2 - y_min * scale

    pen = TTGlyphPen(font.getGlyphSet())
    pen.addComponent(glyph_name, (scale, 0, 0, scale, int(x), int(y)))
    return pen.glyph()


def build_label_glyph(font: TTFont, label: str):
    width = 1000
    glyphs = [glyph_for_char(font, ch) for ch in label]
    advances = [font["hmtx"].metrics[glyph_name][0] for glyph_name in glyphs]
    total_advance = sum(advances)
    scale_x = min(0.64, 900 / total_advance)
    scale_y = 0.86
    label_width = total_advance * scale_x
    x = int((width - label_width) / 2)
    y = 95
    pen = TTGlyphPen(font.getGlyphSet())
    for glyph_name, advance in zip(glyphs, advances):
        pen.addComponent(glyph_name, (scale_x, 0, 0, scale_y, int(x), y))
        x += advance * scale_x
    return pen.glyph()


def add_scaled_control_pictures(font: TTFont) -> None:
    glyph_order = font.getGlyphOrder()
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    vmtx = font.get("vmtx")
    cmap = font.getBestCmap()

    new_metrics = {}
    for codepoint in CONTROL_PICTURE_CODEPOINTS:
        source_glyph = cmap.get(codepoint)
        if source_glyph is None:
            raise ValueError(f"source font lacks control picture U+{codepoint:04X}")
        glyph_name = f"scratchpad_control_{codepoint:04x}"
        if glyph_name not in glyph_order:
            glyph_order.append(glyph_name)
        glyf[glyph_name] = build_scaled_component_glyph(font, source_glyph)
        new_metrics[glyph_name] = (1000, 0)

    font.setGlyphOrder(glyph_order)
    hmtx.metrics.update(new_metrics)
    if vmtx is not None:
        vmtx.metrics.update({name: (1000, 0) for name in new_metrics})
    font["maxp"].numGlyphs = len(glyph_order)

    for table in font["cmap"].tables:
        if table.isUnicode():
            for codepoint in CONTROL_PICTURE_CODEPOINTS:
                table.cmap[codepoint] = f"scratchpad_control_{codepoint:04x}"


def add_pua_glyphs(font: TTFont) -> None:
    glyph_order = font.getGlyphOrder()
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    vmtx = font.get("vmtx")

    new_metrics = {}
    for codepoint, label in PUA_LABELS.items():
        glyph_name = f"scratchpad_{label.lower()}"
        if glyph_name not in glyph_order:
            glyph_order.append(glyph_name)
        glyf[glyph_name] = build_label_glyph(font, label)
        new_metrics[glyph_name] = (1000, 0)

    font.setGlyphOrder(glyph_order)
    hmtx.metrics.update(new_metrics)
    if vmtx is not None:
        vmtx.metrics.update({name: (1000, 0) for name in new_metrics})
    font["maxp"].numGlyphs = len(glyph_order)

    for table in font["cmap"].tables:
        if table.isUnicode():
            for codepoint, label in PUA_LABELS.items():
                table.cmap[codepoint] = f"scratchpad_{label.lower()}"


def main() -> None:
    font = TTFont(SOURCE)
    rename_font(font)
    add_scaled_control_pictures(font)
    add_pua_glyphs(font)
    font.save(TARGET)
    print(f"Wrote {TARGET}")


if __name__ == "__main__":
    main()
