# Upstream

Fucha is a fork of [Mister Download Manager](https://github.com/anefzaoui/misterdownloadmanager).

- Base: `v0.5.2` (`605a0c74e50361afe77bf1721d7172636bd5c57e`, released 2026-11-09).
- Upstream remote: `upstream` (fetch only; do not push to it).
- Fucha-owned paths: `src/`, `src-tauri/`, `package.json`, `vite.config.ts`,
  `index.html`, `tsconfig*.json`, `components.json`, `public/`, `README.md`.
- Keep changes under `crates/` small and upstream-friendly; merge `upstream/main`
  regularly instead of hard-forking.
- Upstream's README is kept as `UPSTREAM.md` so Git's rename detection carries
  upstream README edits through future merges.
