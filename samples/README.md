# Sample models

These files are synthetic geometry authored for ModelView by `scripts/make_samples.py`.
No third-party model files are included.

- `vase_broken.stl` — a ribbed vase with a hollow cavity, one deliberate 6×6 hole
  (24 open edges) and one flipped triangle (reported as 3 flipped edges).
  41,400 triangles, 85.36 cm³ of material.
- `project.3mf` — a two-object print project on two named plates, with Bambu-style
  `model_settings.config` and `project_settings.config` metadata. Watertight.

Regenerate with:

```sh
python3 scripts/make_samples.py
```

The app embeds the vase sample; both files also ship under `crates/modelview-app/assets/samples/`.
