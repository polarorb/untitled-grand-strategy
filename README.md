# Untitled Grand Strategy

A Cold War grand strategy game. January 1st, 1950: the Iron Curtain has
fallen, Mao has won China, the Soviet Union has the bomb — and in six months,
North Korea crosses the 38th parallel.

Real-time-with-pause on an HoI4-scale province map, built in Rust with
[Bevy](https://bevy.org). Inspired by Hearts of Iron IV, but its own game:
escalation and nuclear brinkmanship, influence warfare, espionage, and the
competition of economic systems. Total war is the failure state.

## Running

```sh
cargo run -p ugs-app                            # launch
cargo run -p ugs-app --features fast-compile    # faster iteration builds
cargo test --workspace                          # headless sim tests
```

Requires Rust ≥ 1.95. Space pauses; 1–5 sets game speed.

## Reading order

- `docs/design/vision.md` — what this game is and is not
- `docs/design/systems/` — per-system design docs
- `CLAUDE.md` — architecture, crate boundaries, and the determinism rules

## Regenerating the world map

```sh
./tools/mapgen/fetch-data.sh          # downloads Natural Earth (public domain)
cargo run -p mapgen --release         # writes assets/data + assets/map
```
