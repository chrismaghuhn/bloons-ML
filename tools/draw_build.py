"""Zeichnet die Türme eines Spielprotokolls (Replay) auf ein Bild der Karte.

    python tools/draw_build.py spiel.jsonl --image data/local/calib/monkey_meadow.webp \\
        --data data/local/game_v1_tutorial_medium60.json -o build.png

Gezeichnet wird der Stand am Ende des Protokolls (Position, Turm, Upgrade-Stufe, Reichweite);
dazu eine Zeitleiste "vor Runde N: Aktion". Das Kartenbild ist Spielgrafik und bleibt lokal
(data/local/, gitignored).

Die Abbildung Spiel-Koordinaten -> Bildpixel ist je Karte und Bildausschnitt kalibriert, indem der
Track aus den Daten auf das Bild gelegt wurde (`--check` zeichnet ihn zur Kontrolle mit).
"""
from __future__ import annotations

import argparse
import json

from PIL import Image, ImageDraw, ImageFont

# Karte -> (px je Einheit x, Pixel bei x=0, px je Einheit y, Pixel bei y=0); y zeigt nach unten.
# Monkey Meadow: Screenshot 765×510 ohne UI, Track-Extrempunkte auf die Wegmitte gelegt.
CALIBRATION = {"Tutorial": ((765, 510), (2.583, 385.3, 2.222, 252.8))}
COLORS = {"dart": (40, 90, 200), "boomerang": (200, 120, 20), "bomb": (30, 30, 30),
          "tack": (200, 40, 120), "ice": (40, 170, 210), "glue": (110, 160, 30)}


def towers_and_timeline(lines):
    towers, timeline = {}, []

    def at(x, y):
        # Upgrades/Verkäufe nennen die Turmposition (im f32-Build leicht gerundet): nächster Turm
        return min(towers, key=lambda k: (k[0] - x) ** 2 + (k[1] - y) ** 2)

    for line in lines:
        r = json.loads(line)
        for a in r.get("actions", []):
            kind, v = next(iter(a.items()))
            key = (v["x"], v["y"]) if kind == "place" else at(v["x"], v["y"])
            if kind == "place":
                towers[key] = [v["tower"], [0, 0, 0]]
                timeline.append((r["round"], f"{v['tower']} bauen bei ({v['x']:g}, {v['y']:g})"))
            elif kind == "upgrade":
                t = towers[key]
                t[1][v["path"]] += 1
                timeline.append((r["round"], f"{t[0]} ({key[0]:g}, {key[1]:g}) -> {'-'.join(map(str, t[1]))}"))
            elif kind == "sell":
                t = towers.pop(key)
                timeline.append((r["round"], f"{t[0]} verkaufen"))
    return towers, timeline


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("replay")
    ap.add_argument("--image", required=True)
    ap.add_argument("--data", required=True, help="game_v1-Datei (Reichweiten, Track, Karte)")
    ap.add_argument("--map", default="Tutorial")
    ap.add_argument("-o", "--out", default="build.png")
    ap.add_argument("--check", action="store_true", help="Track aus den Daten mitzeichnen")
    args = ap.parse_args()

    with open(args.data) as f:
        data = json.load(f)
    if args.map not in CALIBRATION:
        ap.error(f"keine Kalibrierung für Karte {args.map!r} (vorhanden: {', '.join(CALIBRATION)})")
    size, (sx, ox, sy, oy) = CALIBRATION[args.map]
    px = lambda x, y: (ox + x * sx, oy + y * sy)  # noqa: E731
    with open(args.replay) as f:
        towers, timeline = towers_and_timeline(f.read().splitlines())

    img = Image.open(args.image).convert("RGB")
    if img.size != size:
        ap.error(f"Kalibrierung gilt für ein {size[0]}×{size[1]}-Bild, dieses ist {img.size[0]}×{img.size[1]}")
    w, h = img.size
    pad = 18 * (len(timeline) + 1)
    out = Image.new("RGB", (w, h + pad), (255, 255, 255))
    out.paste(img, (0, 0))
    d = ImageDraw.Draw(out, "RGBA")
    font = ImageFont.load_default()
    if args.check:
        track = Image.new("RGBA", (w, h))
        ImageDraw.Draw(track).line([px(*p) for p in data["map"]["points"]], fill=(255, 0, 0, 255), width=2)
        out.paste(track, (0, 0), track)  # nur im Kartenbereich
    defs = {t["name"]: t for t in data["towers"]}
    for (x, y), (name, tiers) in towers.items():
        v = next((v for v in defs[name]["variants"] if v["tiers"] == tiers), None)
        cx, cy = px(x, y)
        c = COLORS.get(name, (0, 0, 0))
        if v:
            rx, ry = v["range"] * sx, v["range"] * sy
            d.ellipse([cx - rx, cy - ry, cx + rx, cy + ry], outline=c + (220,), fill=c + (35,), width=2)
        d.ellipse([cx - 9, cy - 9, cx + 9, cy + 9], fill=c, outline=(255, 255, 255), width=2)
        label = f"{name} {'-'.join(map(str, tiers))}"
        d.text((cx + 12, cy - 6), label, fill=(255, 255, 255), font=font, stroke_width=3, stroke_fill=c)
    for i, (rnd, text) in enumerate(timeline):
        d.text((8, h + 6 + 18 * i), f"vor Runde {rnd:>2}: {text}", fill=(0, 0, 0), font=font)
    out.save(args.out)
    print(f"{args.out}: {len(towers)} Türme, {len(timeline)} Aktionen")


if __name__ == "__main__":
    main()
