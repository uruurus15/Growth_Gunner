# Growth Gunner

3レーンを自動で進みながら、ゲートで仲間を増やし、コンテナを撃って武器を強化し、ボスを倒すクラウドランナー風シューティング。Rust + [macroquad](https://github.com/not-fl3/macroquad) 製(Windows)。

## 遊び方
- 移動: `←` `→` / `A` `D`(押している間だけ)、またはマウスのドラッグ
- ゲートで人数を増減、コンテナを撃つと攻撃力・連射・貫通・拡散が強化
- 同じレーンに留まると連射チャージ、ボスの攻撃をギリギリで避けると「ジャスト回避」で攻撃力アップ
- 人数が減ると「救援物資」の箱が出現
- ノーマル / ハイスピード(2倍速)。クリアタイムは名前付きでランキング保存(`ranking.tsv`)

## ビルド
```
cargo run --release
cargo test --release   # ステージのバランス検証(bot)を含む
```
Windows(GNU ツールチェーン)では MinGW-w64 の gcc が PATH に必要です。

## 構成
- `src/sim.rs` ゲームロジック(描画非依存)とバランス検証用 bot
- `src/stages/` データ駆動のステージ定義(`stageN.rs` を作って `all()` に登録)
- `src/render.rs` 描画・演出 / `src/ranking.rs` ランキング

デバッグ用オプション: `--auto --start --jump <z> --units <n> --lv <n> --hi --stage <n> --screen ranking`

## アセット
`assets/` のスプライトは Qwen-Image で生成したものです(Qwen のライセンスに従います)。
