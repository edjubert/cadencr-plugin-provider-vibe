# Connector logos

The approved Cadencr + Mistral Vibe logo: twelve uniform dots connected by
a wave, with the Miaou pixel cat head in the center.

## Files

| Asset | Use |
| --- | --- |
| [logo.svg](logo.svg) | Scalable vector logo with a default size of 512 × 512. |
| [logo-16.png](logo-16.png), [logo-24.png](logo-24.png), [logo-32.png](logo-32.png) | Small interface icons. |
| [logo-48.png](logo-48.png), [logo-64.png](logo-64.png), [logo-128.png](logo-128.png) | Interface and README images. |
| [logo-256.png](logo-256.png), [logo-512.png](logo-512.png), [logo-1024.png](logo-1024.png) | Large images and high-resolution exports. |
| [preview.png](preview.png) | Cadencr comparison and size previews on light and dark backgrounds. |

All PNG logos are square and preserve transparency around the graphite disc.
The twelve contour dots, including the orange dot, share the Cadencr standard
radius of 1.9 on a 48-unit grid.

## Regenerate

The connector's root [icon.svg](../icon.svg) is the source of truth and the
asset referenced by the provider descriptor. The vector and PNG exports are
generated from it:

```bash
python3 scripts/export-logos.py
```

Requires Python 3 and `rsvg-convert` from librsvg (`brew install librsvg` on
macOS). The comparison preview is a separate presentation image.

See [third-party notices](../THIRD_PARTY_NOTICES.md) for sources and licenses.
