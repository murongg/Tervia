# Artwork

## Tervia identity

`tervia.svg` is the single-color split T logo, shared by the native welcome screen and Linux package. The macOS packager renders it to PNG and ICNS. `tervia.ico` contains the Windows export at 16, 24, 32, 48, 64, 128, and 256 pixels and is embedded in the Windows executable.

After changing the SVG, regenerate the Windows export on macOS:

```sh
mkdir -p build
swift scripts/icon.swift build/icon.png assets/tervia.ico
```

## Agent glyphs

Claude, Codex (the OpenAI mark), Gemini, Grok, OpenCode and OpenClaw glyphs are sourced from LobeHub's static icon set:

- https://github.com/lobehub/lobe-icons/blob/master/packages/static-svg/icons/claude.svg
- https://github.com/lobehub/lobe-icons/blob/master/packages/static-svg/icons/openai.svg
- https://github.com/lobehub/lobe-icons/tree/master/packages/static-svg/icons

`hermes.svg` is the Hermes Agent master icon from https://github.com/NousResearch/hermes-agent/blob/main/assets/icon-master.svg with its outer background rectangle removed for use as a small glyph. Its source MIT notice is preserved in `hermes-license.txt`.

Both source MIT notices are copied into the app bundle. SVG bytes are embedded in the native executable; they require no network request or WebView at runtime.
