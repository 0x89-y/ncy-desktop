# Icons

Drop a square PNG named `icon.png` here before running `npm run tauri:build`.
At minimum 256×256, ideally 1024×1024.

To generate the platform-specific icon set (`.ico`, `.icns`, `32x32.png`, …) from
that PNG, run:

```
npm run tauri icon icons/icon.png
```

`npm run tauri:dev` works without these — Tauri only complains for release builds.
