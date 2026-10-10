//! ステージ定義(データ駆動)。
//! 新しいステージは `stageN.rs` を作り、`all()` に登録するだけで追加できる。

mod stage1;
mod stage2;

pub const LANES: usize = 3;
pub const LANE_W: f32 = 3.4;
pub const TRACK_HALF: f32 = LANE_W * LANES as f32 / 2.0;

/// レーン番号 → 中心の x 座標
pub fn lane_x(lane: usize) -> f32 {
    (lane as f32 - (LANES as f32 - 1.0) / 2.0) * LANE_W
}

/// x 座標 → いちばん近いレーン番号
pub fn lane_of(x: f32) -> usize {
    (((x / LANE_W) + (LANES as f32 - 1.0) / 2.0).round().max(0.0) as usize).min(LANES - 1)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GateOp {
    Add(i32),
    Mul(i32),
}

impl GateOp {
    pub fn apply(self, n: i32) -> i32 {
        match self {
            GateOp::Add(a) => (n + a).max(0),
            GateOp::Mul(m) => n.saturating_mul(m).min(9999),
        }
    }

    pub fn label(self) -> String {
        match self {
            GateOp::Add(a) if a >= 0 => format!("+{a}"),
            GateOp::Add(a) => format!("{a}"),
            GateOp::Mul(m) => format!("x{m}"),
        }
    }

    pub fn is_good(self) -> bool {
        match self {
            GateOp::Add(a) => a >= 0,
            GateOp::Mul(m) => m >= 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerUp {
    Damage,
    FireRate,
    Pierce,
    Spread,
}

impl PowerUp {
    pub fn name(self, jp: bool) -> &'static str {
        match (self, jp) {
            (PowerUp::Damage, true) => "攻撃力UP!",
            (PowerUp::FireRate, true) => "連射UP!",
            (PowerUp::Pierce, true) => "貫通UP!",
            (PowerUp::Spread, true) => "拡散UP!",
            (PowerUp::Damage, false) => "DAMAGE UP!",
            (PowerUp::FireRate, false) => "RATE UP!",
            (PowerUp::Pierce, false) => "PIERCE UP!",
            (PowerUp::Spread, false) => "SPREAD UP!",
        }
    }

    pub fn short(self, jp: bool) -> &'static str {
        match (self, jp) {
            (PowerUp::Damage, true) => "攻撃力",
            (PowerUp::FireRate, true) => "連射",
            (PowerUp::Pierce, true) => "貫通",
            (PowerUp::Spread, true) => "拡散",
            (PowerUp::Damage, false) => "DMG",
            (PowerUp::FireRate, false) => "RATE",
            (PowerUp::Pierce, false) => "PIERCE",
            (PowerUp::Spread, false) => "SPREAD",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyKind {
    Grunt,
    Brute,
    Boss,
}

pub struct EnemyStats {
    pub hp: f32,
    pub radius: f32,
    /// 前進速度 (u/s)
    pub speed: f32,
}

impl EnemyKind {
    pub fn stats(self) -> EnemyStats {
        match self {
            EnemyKind::Grunt => EnemyStats { hp: 6.0, radius: 0.32, speed: 3.0 },
            EnemyKind::Brute => EnemyStats { hp: 900.0, radius: 1.0, speed: 1.6 },
            EnemyKind::Boss => EnemyStats { hp: 14000.0, radius: 2.1, speed: 1.4 },
        }
    }
}

#[derive(Clone, Debug)]
pub enum EventKind {
    /// レーンごとのゲート(長さ LANES)
    Gates { ops: [GateOp; LANES] },
    /// 撃って壊すとパワーアップを得られるコンテナ
    Crate { lane: usize, hp: f32, reward: PowerUp },
    /// 敵の群れ。`hp_mul` で体力倍率を調整
    Swarm { kind: EnemyKind, x: f32, width: f32, depth: f32, count: u32, hp_mul: f32 },
    /// ボス(ステージ終端で戦闘になる)
    Boss { hp_mul: f32 },
}

#[derive(Clone, Debug)]
pub struct Event {
    pub z: f32,
    pub kind: EventKind,
}

/// ボスの攻撃パターン(危険レーンの出し方)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossPattern {
    /// 安全レーンを 1 つだけ残して一斉に叩く
    Safe,
    /// 左から右へ順番に 1 レーンずつ叩く
    SweepLR,
    /// 右から左へ順番に 1 レーンずつ叩く
    SweepRL,
    /// 外側 2 レーン → 真ん中の順に叩く
    Pincer,
    /// 安全レーンを変えながら 2 連続で叩く
    Shift,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// 海の色(上→下)
    pub sea: [f32; 3],
    /// 路面の色
    pub road: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Stage {
    pub name: &'static str,
    pub theme: Theme,
    /// 雑魚のスプライト番号(2 種類)と、ボスのスプライト番号
    pub grunt_tex: [usize; 2],
    pub boss_tex: usize,
    /// ボスの攻撃間隔の倍率(小さいほど速い)
    pub boss_tempo: f32,
    /// ボスが使う攻撃パターン(順番はランダム)
    pub boss_patterns: Vec<BossPattern>,
    pub start_units: i32,
    /// ここまで走るとボス戦(停止)
    pub finish_z: f32,
    /// z 昇順
    pub events: Vec<Event>,
}

pub fn all() -> Vec<Stage> {
    vec![stage1::build(), stage2::build()]
}

// ── ステージ定義用の共通ヘルパー ──

pub(crate) fn gates(z: f32, a: GateOp, b: GateOp, c: GateOp) -> Event {
    Event { z, kind: EventKind::Gates { ops: [a, b, c] } }
}

pub(crate) fn pcrate(z: f32, lane: usize, hp: f32, reward: PowerUp) -> Event {
    Event { z, kind: EventKind::Crate { lane, hp, reward } }
}

/// 指定レーンそれぞれに `count` 体ずつ配置
pub(crate) fn swarm_hp(ev: &mut Vec<Event>, z: f32, kind: EnemyKind, lanes: &[usize], count: u32, depth: f32, hp_mul: f32) {
    for &l in lanes {
        ev.push(Event { z, kind: EventKind::Swarm { kind, x: lane_x(l), width: LANE_W - 0.8, depth, count, hp_mul } });
    }
}
