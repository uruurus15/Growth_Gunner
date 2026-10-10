//! ゲームロジック(描画・入力に非依存。テストからヘッドレスで回せる)。
//! 演出(パーティクル・揺れ・浮き文字)は `events` を見て render 側で作る。

use crate::stages::{lane_of, lane_x, BossPattern, EnemyKind, EventKind, GateOp, PowerUp, Stage, LANES};

pub use crate::stages::TRACK_HALF;

pub const RUN_SPEED: f32 = 4.0;
pub const SPAWN_AHEAD: f32 = 26.0;
const AGGRO: f32 = 14.0;
const LATERAL_CHASE: f32 = 0.7;
const BULLET_SPEED: f32 = 20.0;
const BULLET_LIFE: f32 = 1.1;
const MAX_SHOTS: i32 = 20;
const PLAYER_LATERAL_SPEED: f32 = 16.0;
const STEER_SPEED: f32 = 9.0;
/// 描画・当たり判定で使う隊列の最大表示数
pub const MAX_DRAWN_UNITS: i32 = 120;

// ボス
const BOSS_HOLD_OFFSET: f32 = 7.0;
const BOSS_IDLE: f32 = 2.4;
pub const BOSS_WINDUP: f32 = 1.1;
/// 連続攻撃の最初の着弾までと、次の着弾までの間隔(ハイスピードでも避けられる余裕)
const SWEEP_START: f32 = 1.2;
const SWEEP_GAP: f32 = 0.95;
const COMBO_GAP: f32 = 1.15;
/// 増援の箱: 人数がこの数以下の状態が一定時間続くと出現する
const REINFORCE_THRESHOLD: i32 = 50;
const REINFORCE_DELAY: f32 = 10.0;
const REINFORCE_HP: f32 = 100.0;
const REINFORCE_UNITS: i32 = 30;
const REINFORCE_SPEED: f32 = 1.5;
/// ギリギリ回避の判定幅(着弾の何秒前まで危険レーンにいたか)
const JUST_WINDOW: f32 = 0.35;
const JUST_BOOST_TIME: f32 = 0.5;
const JUST_BOOST_MULT: f32 = 3.0;
/// 同じレーンに留まるとチャージが貯まる(満タンまでの秒数と連射ボーナス)
const CHARGE_TIME: f32 = 3.0;
const CHARGE_RATE_BONUS: f32 = 0.8;
const BOSS_RECOVER: f32 = 0.6;
const BOSS_SUMMON_INTERVAL: f32 = 8.0;
const BOSS_SUMMON_COUNT: u32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Playing,
    Won,
    Lost,
}

/// render が毎フレーム drain する出来事
#[derive(Clone, Debug)]
pub enum GameEvent {
    Volley,
    Kill { x: f32, z: f32, kind: EnemyKind },
    UnitsLost { x: f32, z: f32, n: i32 },
    CrateBroken { x: f32, z: f32, reward: PowerUp },
    ReinforceSpawn,
    Reinforce { x: f32, z: f32, n: i32 },
    Gate { x: f32, z: f32, op: GateOp, delta: i32 },
    BossIntro,
    BossWindup,
    BossSlam { hit: bool, lost: i32, danger: [bool; LANES] },
    JustDodge,
}

#[derive(Clone, Debug)]
pub struct Weapon {
    pub damage_lv: u32,
    pub rate_lv: u32,
    pub pierce_lv: u32,
    pub spread_lv: u32,
}

impl Weapon {
    pub fn new() -> Self {
        Weapon { damage_lv: 0, rate_lv: 0, pierce_lv: 0, spread_lv: 0 }
    }
    /// 1 ユニットあたりの威力
    pub fn damage(&self) -> f32 {
        1.0 + 0.6 * self.damage_lv as f32
    }
    /// 毎秒の一斉射回数
    pub fn rate(&self) -> f32 {
        2.5 * (1.0 + 0.35 * self.rate_lv as f32)
    }
    pub fn pierce(&self) -> u32 {
        1 + self.pierce_lv
    }
    pub fn streams(&self) -> u32 {
        1 + self.spread_lv
    }
}

pub struct Gate {
    pub z: f32,
    pub ops: [GateOp; LANES],
    pub taken: Option<usize>,
}

pub struct Crate {
    pub x: f32,
    pub z: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub reward: PowerUp,
    pub hit_timer: f32,
    /// 0 以外なら増援の箱(壊すとこの数だけ味方が増える)
    pub reinforce: i32,
    /// z 方向の移動速度(増援の箱は手前へ流れてくる)
    pub vz: f32,
}

pub struct Enemy {
    pub id: u32,
    pub kind: EnemyKind,
    pub x: f32,
    pub z: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub aggro: bool,
    pub radius: f32,
    pub hit_timer: f32,
}

pub struct Bullet {
    pub x: f32,
    pub z: f32,
    pub vx: f32,
    pub vz: f32,
    dmg: f32,
    pierce: u32,
    life: f32,
    hit: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BossPhase {
    Idle,
    Attack,
    Recover,
}

/// 1 回の叩きつけ(危険レーンと着弾時刻)
pub struct Strike {
    pub danger: [bool; LANES],
    /// 攻撃開始からの経過時間でこの時刻に着弾する
    pub slam_at: f32,
    /// 予告表示の開始時刻
    pub warn_from: f32,
    pub done: bool,
    /// 着弾後の余韻(演出用)
    pub after: f32,
    /// 直近に隊列が危険レーンに掛かっていた時刻(-1 = まだ)
    touched_at: f32,
}

pub struct BossAi {
    pub phase: BossPhase,
    pub t: f32,
    /// 攻撃開始からの経過時間
    pub clock: f32,
    pub strikes: Vec<Strike>,
    summon_timer: f32,
    /// ボス戦の経過時間(増援が増える)
    pub fight_time: f32,
    last_pattern: usize,
}

impl BossAi {
    /// 予告中(まだ着弾していない)の叩きつけがあるか
    pub fn warning(&self) -> bool {
        self.strikes.iter().any(|s| !s.done && self.clock >= s.warn_from)
    }
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
}

pub struct World {
    pub stage: Stage,
    pub state: State,
    pub time: f32,
    pub player_x: f32,
    pub player_z: f32,
    pub target_x: f32,
    pub units: i32,
    pub weapon: Weapon,
    pub bullets: Vec<Bullet>,
    pub enemies: Vec<Enemy>,
    pub crates: Vec<Crate>,
    pub gates: Vec<Gate>,
    pub events: Vec<GameEvent>,
    pub boss_ai: BossAi,
    /// 同じレーンに留まった長さ(0..1)
    pub charge: f32,
    charge_lane: usize,
    /// ジャスト回避の攻撃力アップの残り時間
    pub boost_t: f32,
    /// 人数が少ない状態が続いている時間(増援の箱の出現用)
    low_timer: f32,
    pub boss_spawned: bool,
    pub boss_intro_done: bool,
    pub kills: u32,
    pub units_lost: i32,
    pub peak_units: i32,
    next_event: usize,
    next_id: u32,
    volley_timer: f32,
    rng: Rng,
}

impl World {
    pub fn new(stage: Stage) -> Self {
        let units = stage.start_units;
        let boss_idle = BOSS_IDLE * stage.boss_tempo;
        World {
            stage,
            state: State::Playing,
            time: 0.0,
            player_x: 0.0,
            player_z: 0.0,
            target_x: 0.0,
            units,
            weapon: Weapon::new(),
            bullets: Vec::new(),
            enemies: Vec::new(),
            crates: Vec::new(),
            gates: Vec::new(),
            events: Vec::new(),
            boss_ai: BossAi { phase: BossPhase::Idle, t: boss_idle, clock: 0.0, strikes: Vec::new(), summon_timer: BOSS_SUMMON_INTERVAL, fight_time: 0.0, last_pattern: usize::MAX },
            charge: 0.0,
            charge_lane: 1,
            boost_t: 0.0,
            low_timer: 0.0,
            boss_spawned: false,
            boss_intro_done: false,
            kills: 0,
            units_lost: 0,
            peak_units: units,
            next_event: 0,
            next_id: 1,
            volley_timer: 0.0,
            rng: Rng(0x9E37_79B9),
        }
    }

    /// 隊列の半径(接触判定にも使う。1 レーンに収まる)
    pub fn squad_radius(&self) -> f32 {
        0.3 + 0.11 * (self.units.min(MAX_DRAWN_UNITS) as f32).sqrt()
    }

    pub fn at_finish(&self) -> bool {
        self.player_z >= self.stage.finish_z
    }

    pub fn boss(&self) -> Option<&Enemy> {
        self.enemies.iter().find(|e| e.kind == EnemyKind::Boss)
    }

    /// 目標位置(ワールド x)。隊列はそこへ滑らかに寄る
    pub fn set_target_x(&mut self, x: f32) {
        let lim = TRACK_HALF - 0.7;
        self.target_x = x.clamp(-lim, lim);
    }

    /// 押している間だけ動く(dir: -1..1、dt はゲーム時間)
    pub fn steer(&mut self, dir: f32, dt: f32) {
        self.set_target_x(self.target_x + dir * STEER_SPEED * dt);
    }

    /// bot 用: レーン中心を狙う
    pub fn set_lane(&mut self, lane: usize) {
        self.set_target_x(lane_x(lane.min(LANES - 1)));
    }

    pub fn step(&mut self, dt: f32) {
        if self.state != State::Playing {
            return;
        }
        self.time += dt;
        self.spawn_events();
        self.move_player(dt);
        self.check_gates();
        self.update_charge(dt);
        self.update_reinforce(dt);
        self.shoot(dt);
        self.update_bullets(dt);
        self.update_enemies(dt);
        self.update_boss(dt);
        self.cleanup(dt);
        self.peak_units = self.peak_units.max(self.units);

        if self.units <= 0 {
            self.units = 0;
            self.state = State::Lost;
        } else if self.boss_spawned && self.boss().is_none() {
            self.state = State::Won;
        }
    }

    fn alloc_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    fn spawn_enemy(&mut self, kind: EnemyKind, x: f32, z: f32, hp_mul: f32, aggro: bool) {
        let s = kind.stats();
        let hp = s.hp * hp_mul;
        let id = self.alloc_id();
        self.enemies.push(Enemy {
            id,
            kind,
            x: x.clamp(-TRACK_HALF + s.radius, TRACK_HALF - s.radius),
            z,
            hp,
            max_hp: hp,
            aggro,
            radius: s.radius,
            hit_timer: 0.0,
        });
    }

    fn spawn_events(&mut self) {
        while self.next_event < self.stage.events.len() {
            let ev = self.stage.events[self.next_event].clone();
            if self.player_z + SPAWN_AHEAD < ev.z {
                break;
            }
            self.next_event += 1;
            match ev.kind {
                EventKind::Gates { ops } => {
                    self.gates.push(Gate { z: ev.z, ops, taken: None });
                }
                EventKind::Crate { lane, hp, reward } => {
                    self.crates.push(Crate { x: lane_x(lane), z: ev.z, hp, max_hp: hp, reward, hit_timer: 0.0, reinforce: 0, vz: 0.0 });
                }
                EventKind::Swarm { kind, x, width, depth, count, hp_mul } => {
                    for _ in 0..count {
                        let ex = x + self.rng.range(-width / 2.0, width / 2.0);
                        let ez = ev.z + self.rng.range(0.0, depth);
                        self.spawn_enemy(kind, ex, ez, hp_mul, false);
                    }
                }
                EventKind::Boss { hp_mul } => {
                    self.spawn_enemy(EnemyKind::Boss, 0.0, ev.z, hp_mul, false);
                    self.boss_spawned = true;
                }
            }
        }
    }

    fn move_player(&mut self, dt: f32) {
        let dx = self.target_x - self.player_x;
        let max = PLAYER_LATERAL_SPEED * dt;
        self.player_x += dx.clamp(-max, max);

        // ボス戦以外では止まらない
        if !self.at_finish() {
            self.player_z = (self.player_z + RUN_SPEED * dt).min(self.stage.finish_z);
        }
    }

    fn check_gates(&mut self) {
        let (px, pz) = (self.player_x, self.player_z);
        let lane = lane_of(px);
        let mut evs = Vec::new();
        for g in self.gates.iter_mut() {
            if g.taken.is_none() && pz >= g.z {
                g.taken = Some(lane);
                let op = g.ops[lane];
                let before = self.units;
                self.units = op.apply(self.units);
                let diff = self.units - before;
                if diff < 0 {
                    self.units_lost += -diff;
                }
                evs.push(GameEvent::Gate { x: px, z: pz, op, delta: diff });
            }
        }
        self.events.extend(evs);
    }

    fn has_target_ahead(&self) -> bool {
        let lo = self.player_z - 0.5;
        let hi = self.player_z + BULLET_SPEED * BULLET_LIFE;
        self.enemies.iter().any(|e| e.z > lo && e.z < hi) || self.crates.iter().any(|c| c.z > lo && c.z < hi)
    }

    fn shoot(&mut self, dt: f32) {
        self.volley_timer += dt;
        let interval = 1.0 / (self.weapon.rate() * (1.0 + CHARGE_RATE_BONUS * self.charge));
        if self.volley_timer < interval {
            return;
        }
        if !self.has_target_ahead() {
            self.volley_timer = interval;
            return;
        }
        self.volley_timer -= interval;
        self.events.push(GameEvent::Volley);

        let shots = self.units.clamp(1, MAX_SHOTS);
        let boost = if self.boost_t > 0.0 { JUST_BOOST_MULT } else { 1.0 };
        let dmg = self.weapon.damage() * boost * self.units as f32 / shots as f32;
        let streams = self.weapon.streams() as i32;
        let r = self.squad_radius();

        // エイム対象: 前方の敵(近いものを優先して最大 12 体)
        let (px, pz) = (self.player_x, self.player_z);
        let mut cands: Vec<(f32, f32)> = self
            .enemies
            .iter()
            .filter(|e| e.hp > 0.0 && e.z > pz + 0.5 && e.z < pz + BULLET_SPEED * BULLET_LIFE * 0.9)
            .map(|e| (e.x, e.z))
            .collect();
        cands.sort_by(|a, b| (a.1 - pz).partial_cmp(&(b.1 - pz)).unwrap());
        cands.truncate(12);

        for i in 0..shots {
            let lane = (i % streams) as f32 - (streams - 1) as f32 / 2.0;
            let ox = self.rng.range(-r, r) * 0.8;
            let bx = px + ox;
            let bz = pz + self.rng.range(0.0, r);
            let mut ang = lane * 0.10;
            if !cands.is_empty() {
                let (tx, tz) = cands[(self.rng.next() * cands.len() as f32) as usize % cands.len()];
                ang += (tx - bx).atan2(tz - bz).clamp(-0.6, 0.6);
            }
            self.bullets.push(Bullet {
                x: bx,
                z: bz,
                vx: ang.sin() * BULLET_SPEED,
                vz: ang.cos() * BULLET_SPEED,
                dmg,
                pierce: self.weapon.pierce(),
                life: BULLET_LIFE,
                hit: Vec::new(),
            });
        }
    }

    fn update_bullets(&mut self, dt: f32) {
        let mut rewards: Vec<(f32, f32, PowerUp, i32)> = Vec::new();
        let mut bullets = std::mem::take(&mut self.bullets);
        for b in bullets.iter_mut() {
            b.x += b.vx * dt;
            b.z += b.vz * dt;
            b.life -= dt;

            // コンテナ(弾はそこで止まる)
            let mut stopped = false;
            for c in self.crates.iter_mut() {
                if c.hp > 0.0 && (b.x - c.x).abs() < 1.2 && (b.z - c.z).abs() < 0.8 {
                    c.hp -= b.dmg;
                    c.hit_timer = 0.1;
                    if c.hp <= 0.0 {
                        rewards.push((c.x, c.z, c.reward, c.reinforce));
                    }
                    stopped = true;
                    break;
                }
            }
            if stopped {
                b.life = 0.0;
                continue;
            }

            for e in self.enemies.iter_mut() {
                if e.hp <= 0.0 || b.hit.contains(&e.id) {
                    continue;
                }
                if (b.x - e.x).abs() < e.radius + 0.12 && (b.z - e.z).abs() < e.radius + 0.2 {
                    e.hp -= b.dmg;
                    e.hit_timer = 0.1;
                    b.hit.push(e.id);
                    b.pierce -= 1;
                    if b.pierce == 0 {
                        b.life = 0.0;
                        break;
                    }
                }
            }
        }
        bullets.retain(|b| b.life > 0.0);
        self.bullets = bullets;

        for (x, z, r, reinforce) in rewards {
            if reinforce > 0 {
                self.units += reinforce;
                self.events.push(GameEvent::Reinforce { x, z, n: reinforce });
            } else {
                self.apply_powerup(r);
                self.events.push(GameEvent::CrateBroken { x, z, reward: r });
            }
        }
    }

    pub fn apply_powerup(&mut self, p: PowerUp) {
        match p {
            PowerUp::Damage => self.weapon.damage_lv += 1,
            PowerUp::FireRate => self.weapon.rate_lv += 1,
            PowerUp::Pierce => self.weapon.pierce_lv += 1,
            PowerUp::Spread => self.weapon.spread_lv += 1,
        }
    }

    fn lose_units(&mut self, x: f32, z: f32, n: i32) {
        let n = n.min(self.units);
        if n > 0 {
            self.units -= n;
            self.units_lost += n;
            self.events.push(GameEvent::UnitsLost { x, z, n });
        }
    }

    fn update_enemies(&mut self, dt: f32) {
        let (px, pz) = (self.player_x, self.player_z);
        let r = self.squad_radius();
        let finished = self.at_finish();
        let mut trades: Vec<(f32, f32, i32)> = Vec::new();

        for e in self.enemies.iter_mut() {
            e.hit_timer = (e.hit_timer - dt).max(0.0);
            if e.hp <= 0.0 {
                continue;
            }
            if !e.aggro {
                if e.kind != EnemyKind::Boss && e.z - pz < AGGRO {
                    e.aggro = true;
                } else if e.kind == EnemyKind::Boss && finished {
                    e.aggro = true;
                }
            }

            if e.kind == EnemyKind::Boss {
                // ボスは定位置まで寄って、そこから動かない
                let hold = self.stage.finish_z + BOSS_HOLD_OFFSET;
                if e.aggro && e.z > hold {
                    e.z = (e.z - e.kind.stats().speed * dt).max(hold);
                }
                continue;
            }

            if e.aggro {
                let speed = e.kind.stats().speed;
                e.z -= speed * dt;
                let lat = (px - e.x).clamp(-LATERAL_CHASE * dt, LATERAL_CHASE * dt);
                e.x += lat;
            }

            // 接触: 1 回のトレード(敵は倒れ、味方が減る)
            let dx = px - e.x;
            let dz = pz - e.z;
            if dx * dx + dz * dz < (r + e.radius) * (r + e.radius) {
                let cost = match e.kind {
                    EnemyKind::Grunt => 1,
                    EnemyKind::Brute => ((e.hp / 45.0).ceil() as i32).max(3),
                    EnemyKind::Boss => 0,
                };
                trades.push((e.x, e.z, cost));
                e.hp = 0.0;
            }
        }
        for (x, z, cost) in trades {
            self.lose_units(x, z, cost);
        }
    }

    /// 隊列(中心と端)が危険レーンに掛かっているか
    fn squad_in_danger(&self, danger: &[bool; LANES]) -> bool {
        let r = self.squad_radius() * 0.7;
        [self.player_x - r, self.player_x, self.player_x + r].iter().any(|&x| danger[lane_of(x)])
    }

    /// パターンから叩きつけの列を作る(着弾時刻は攻撃開始からの秒数)
    fn build_strikes(&mut self, pattern: BossPattern) -> Vec<Strike> {
        let mask = |lanes: &[usize]| {
            let mut m = [false; LANES];
            for &l in lanes {
                m[l] = true;
            }
            m
        };
        let strike = |danger: [bool; LANES], slam_at: f32| Strike {
            danger,
            slam_at,
            warn_from: (slam_at - BOSS_WINDUP).max(0.0),
            done: false,
            after: 0.0,
            touched_at: -1.0,
        };
        let safe_mask = |safe: usize| {
            let mut m = [true; LANES];
            m[safe] = false;
            m
        };
        match pattern {
            BossPattern::Safe => {
                let safe = (self.rng.next() * LANES as f32) as usize % LANES;
                vec![strike(safe_mask(safe), BOSS_WINDUP)]
            }
            BossPattern::SweepLR => (0..LANES).map(|l| strike(mask(&[l]), SWEEP_START + SWEEP_GAP * l as f32)).collect(),
            BossPattern::SweepRL => (0..LANES).map(|l| strike(mask(&[LANES - 1 - l]), SWEEP_START + SWEEP_GAP * l as f32)).collect(),
            BossPattern::Pincer => vec![strike(mask(&[0, LANES - 1]), SWEEP_START), strike(mask(&[LANES / 2]), SWEEP_START + COMBO_GAP)],
            BossPattern::Shift => {
                let a = (self.rng.next() * LANES as f32) as usize % LANES;
                let b = (a + 1 + (self.rng.next() * (LANES - 1) as f32) as usize % (LANES - 1)) % LANES;
                vec![strike(safe_mask(a), SWEEP_START), strike(safe_mask(b), SWEEP_START + COMBO_GAP)]
            }
        }
    }

    fn update_boss(&mut self, dt: f32) {
        let Some(boss) = self.enemies.iter().find(|e| e.kind == EnemyKind::Boss) else { return };
        if !boss.aggro {
            return;
        }
        let (bx, bz) = (boss.x, boss.z);
        if !self.boss_intro_done {
            self.boss_intro_done = true;
            self.events.push(GameEvent::BossIntro);
        }

        self.boss_ai.fight_time += dt;
        match self.boss_ai.phase {
            BossPhase::Idle => {
                self.boss_ai.t -= dt;
                if self.boss_ai.t <= 0.0 {
                    // 前回と違うパターンをランダムに選ぶ
                    let n = self.stage.boss_patterns.len().max(1);
                    let mut idx = (self.rng.next() * n as f32) as usize % n;
                    if n > 1 && idx == self.boss_ai.last_pattern {
                        idx = (idx + 1) % n;
                    }
                    self.boss_ai.last_pattern = idx;
                    let pattern = self.stage.boss_patterns.get(idx).copied().unwrap_or(BossPattern::Safe);
                    self.boss_ai.strikes = self.build_strikes(pattern);
                    self.boss_ai.clock = 0.0;
                    self.boss_ai.phase = BossPhase::Attack;
                    self.events.push(GameEvent::BossWindup);
                }
            }
            BossPhase::Attack => {
                self.boss_ai.clock += dt;
                let clock = self.boss_ai.clock;
                let mut strikes = std::mem::take(&mut self.boss_ai.strikes);
                for s in strikes.iter_mut() {
                    if s.done {
                        s.after = (s.after - dt).max(0.0);
                        continue;
                    }
                    if clock < s.warn_from {
                        continue;
                    }
                    let inside = self.squad_in_danger(&s.danger);
                    if inside {
                        s.touched_at = clock;
                    }
                    if clock >= s.slam_at {
                        s.done = true;
                        s.after = 0.35;
                        let mut lost = 0;
                        if inside {
                            lost = ((self.units as f32 * 0.35).ceil() as i32 + 4).min(self.units);
                            self.lose_units(self.player_x, self.player_z, lost);
                        } else if s.touched_at >= 0.0 && s.slam_at - s.touched_at <= JUST_WINDOW {
                            self.boost_t = JUST_BOOST_TIME;
                            self.events.push(GameEvent::JustDodge);
                        }
                        self.events.push(GameEvent::BossSlam { hit: inside, lost, danger: s.danger });
                    }
                }
                let all_done = strikes.iter().all(|s| s.done);
                self.boss_ai.strikes = strikes;
                if all_done {
                    self.boss_ai.phase = BossPhase::Recover;
                    self.boss_ai.t = BOSS_RECOVER;
                }
            }
            BossPhase::Recover => {
                self.boss_ai.t -= dt;
                for s in self.boss_ai.strikes.iter_mut() {
                    s.after = (s.after - dt).max(0.0);
                }
                if self.boss_ai.t <= 0.0 {
                    self.boss_ai.strikes.clear();
                    self.boss_ai.phase = BossPhase::Idle;
                    self.boss_ai.t = BOSS_IDLE * self.stage.boss_tempo;
                }
            }
        }

        self.boss_ai.summon_timer -= dt;
        if self.boss_ai.summon_timer <= 0.0 {
            self.boss_ai.summon_timer = BOSS_SUMMON_INTERVAL * self.stage.boss_tempo.max(0.5);
            let count = BOSS_SUMMON_COUNT + (self.boss_ai.fight_time / 6.0) as u32 * 3;
            for _ in 0..count {
                let x = bx + self.rng.range(-4.0, 4.0);
                let z = bz - self.rng.range(1.0, 3.0);
                self.spawn_enemy(EnemyKind::Grunt, x, z, 1.0, true);
            }
        }
    }

    /// 人数が少ない状態が続いたら、増援の箱を前方から流す
    fn update_reinforce(&mut self, dt: f32) {
        for c in self.crates.iter_mut() {
            c.z += c.vz * dt;
        }
        if self.units > 0 && self.units <= REINFORCE_THRESHOLD {
            self.low_timer += dt;
        } else {
            self.low_timer = 0.0;
        }
        if self.low_timer >= REINFORCE_DELAY && !self.crates.iter().any(|c| c.reinforce > 0) {
            self.low_timer = 0.0;
            let lane = (self.rng.next() * LANES as f32) as usize % LANES;
            self.crates.push(Crate {
                x: lane_x(lane),
                z: self.player_z + 17.0,
                hp: REINFORCE_HP,
                max_hp: REINFORCE_HP,
                reward: PowerUp::Damage,
                hit_timer: 0.0,
                reinforce: REINFORCE_UNITS,
                vz: -REINFORCE_SPEED,
            });
            self.events.push(GameEvent::ReinforceSpawn);
        }
    }

    /// 同じレーンに留まるほどチャージが貯まり、連射が速くなる
    fn update_charge(&mut self, dt: f32) {
        let lane = lane_of(self.player_x);
        if lane != self.charge_lane {
            self.charge_lane = lane;
            self.charge = 0.0;
        } else {
            self.charge = (self.charge + dt / CHARGE_TIME).min(1.0);
        }
        self.boost_t = (self.boost_t - dt).max(0.0);
    }

    fn cleanup(&mut self, dt: f32) {
        for c in self.crates.iter_mut() {
            c.hit_timer = (c.hit_timer - dt).max(0.0);
        }
        let mut killed = Vec::new();
        self.enemies.retain(|e| {
            if e.hp <= 0.0 {
                killed.push((e.x, e.z, e.kind));
                false
            } else {
                true
            }
        });
        self.kills += killed.len() as u32;
        for (x, z, kind) in killed {
            self.events.push(GameEvent::Kill { x, z, kind });
        }
        let pz = self.player_z;
        self.crates.retain(|c| c.hp > 0.0 && c.z > pz - 6.0);
        self.gates.retain(|g| g.z > pz - 6.0);
        self.enemies.retain(|e| e.z > pz - 12.0);
    }
}

/// 賢いプレイヤーの方針。テストのバランス検証と、`--auto` のデバッグ実行で使う。
pub mod bot {
    use super::*;

    fn best_gate_lane(w: &World, g: &Gate) -> usize {
        (0..LANES).max_by_key(|&l| g.ops[l].apply(w.units)).unwrap()
    }

    fn next_gate(w: &World) -> Option<&Gate> {
        w.gates.iter().filter(|g| g.taken.is_none()).min_by(|a, b| a.z.partial_cmp(&b.z).unwrap())
    }

    /// ボスの叩きつけ予告を避ける。なければ None
    /// 直近の着弾から順に「連続で安全」な本数が最も長いレーンを選ぶ。
    pub fn dodge(w: &World) -> Option<usize> {
        if w.boss_ai.phase != BossPhase::Attack {
            return None;
        }
        let mut pending: Vec<&Strike> = w.boss_ai.strikes.iter().filter(|s| !s.done).collect();
        if pending.is_empty() {
            return None;
        }
        pending.sort_by(|a, b| a.slam_at.partial_cmp(&b.slam_at).unwrap());
        let cur = lane_of(w.player_x);
        (0..LANES).max_by_key(|&l| {
            let safe_run = pending.iter().take_while(|s| !s.danger[l]).count();
            // 同点なら今いるレーンに近い方
            (safe_run, -((l as i32 - cur as i32).abs()))
        })
    }

    /// ゲートも箱も賢く選ぶ
    pub fn smart(w: &World) -> usize {
        if let Some(l) = dodge(w) {
            return l;
        }
        let gate = next_gate(w);
        let crate_ = w.crates.iter().filter(|c| c.z > w.player_z).min_by(|a, b| a.z.partial_cmp(&b.z).unwrap());
        if let Some(g) = gate {
            if g.z - w.player_z < 9.0 {
                return best_gate_lane(w, g);
            }
        }
        if let Some(c) = crate_ {
            if c.z - w.player_z < 22.0 {
                return lane_of(c.x);
            }
        }
        if let Some(g) = gate {
            if g.z - w.player_z < 22.0 {
                return best_gate_lane(w, g);
            }
        }
        1
    }

    /// ゲートだけ最適に選び、箱は無視
    #[allow(dead_code)]
    pub fn gates_only(w: &World) -> usize {
        if let Some(l) = dodge(w) {
            return l;
        }
        match next_gate(w) {
            Some(g) if g.z - w.player_z < 22.0 => best_gate_lane(w, g),
            _ => 1,
        }
    }
}

impl World {
    /// デバッグ用: 指定地点までワープ(通り過ぎるイベントはスキップ)
    pub fn debug_jump(&mut self, z: f32, units: i32, levels: u32) {
        self.player_z = z;
        self.units = units;
        self.peak_units = units;
        self.weapon = Weapon { damage_lv: levels, rate_lv: levels, pierce_lv: levels, spread_lv: levels };
        while self.next_event < self.stage.events.len() && self.stage.events[self.next_event].z < z {
            self.next_event += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::bot::*;
    use super::*;
    use crate::stages;

    /// bot の方針: 世界を見て目標レーンを返す
    type Policy = fn(&World) -> usize;

    struct Outcome {
        state: State,
        units: i32,
        peak: i32,
        t: f32,
        z: f32,
    }

    fn run(stage: usize, policy: Policy, no_upgrades: bool) -> Outcome {
        let mut w = World::new(stages::all().remove(stage));
        let dt = 1.0 / 60.0;
        for _ in 0..(60 * 400) {
            w.set_lane(policy(&w));
            if no_upgrades {
                w.weapon = Weapon::new();
            }
            w.step(dt);
            w.events.clear();
            if w.state != State::Playing {
                break;
            }
        }
        Outcome { state: w.state, units: w.units, peak: w.peak_units, t: w.time, z: w.player_z }
    }

    fn lane0(_: &World) -> usize {
        0
    }
    fn lane1(_: &World) -> usize {
        1
    }
    fn lane2(_: &World) -> usize {
        2
    }

    #[test]
    fn smart_player_wins() {
        for s in 0..stages::all().len() {
            let o = run(s, smart, false);
            println!("stage{} smart: {:?} units={} peak={} t={:.1} z={:.0}", s + 1, o.state, o.units, o.peak, o.t, o.z);
            assert_eq!(o.state, State::Won, "stage {} should be winnable", s + 1);
            assert!(o.t < 150.0, "stage {} should not drag on", s + 1);
        }
    }

    #[test]
    fn careless_players_lose() {
        for s in 0..stages::all().len() {
            for (name, p, nu) in [
                ("lane0", lane0 as Policy, false),
                ("lane1", lane1, false),
                ("lane2", lane2, false),
                ("gates_only", gates_only, false),
                ("smart_no_upgrades", smart, true),
            ] {
                let o = run(s, p, nu);
                println!("stage{} {name}: {:?} units={} peak={} t={:.1} z={:.0}", s + 1, o.state, o.units, o.peak, o.t, o.z);
                // 負けるか、勝てても現実的でないほど長引く(150 秒超。賢いプレイは 150 秒未満で勝てる)こと
                let ok = o.state == State::Lost || o.t > 150.0;
                assert!(ok, "stage {} {name} should lose or drag on (state {:?}, t {:.0})", s + 1, o.state, o.t);
            }
        }
    }

    /// 危険レーンから着弾ギリギリで抜けると JustDodge、早めに抜けても何も起きない
    #[test]
    fn just_dodge_only_when_late() {
        for (late, expect) in [(true, true), (false, false)] {
            let mut w = World::new(stages::all().remove(0));
            w.debug_jump(262.0, 100, 0);
            let mut just = false;
            let mut hit = false;
            for _ in 0..(60 * 20) {
                if w.boss_ai.phase == BossPhase::Attack {
                    let s = &w.boss_ai.strikes[0];
                    let safe = (0..LANES).find(|&l| !s.danger[l]).unwrap();
                    // 安全レーンの隣(危険レーン)に居て、直前に飛び込む
                    let danger = if safe > 0 { safe - 1 } else { safe + 1 };
                    if !late || w.boss_ai.clock >= s.slam_at - 0.3 {
                        w.set_lane(safe);
                    } else {
                        w.set_lane(danger);
                    }
                } else {
                    w.set_lane(1);
                }
                w.step(1.0 / 60.0);
                for e in w.events.drain(..) {
                    match e {
                        GameEvent::JustDodge => just = true,
                        GameEvent::BossSlam { hit: h, .. } => hit |= h,
                        _ => {}
                    }
                }
                if just || hit || w.boss_ai.phase == BossPhase::Recover {
                    break;
                }
            }
            assert!(!hit, "late={late}: should not be hit");
            assert_eq!(just, expect, "late={late}");
            if expect {
                assert!(w.boost_t > 0.0);
            }
        }
    }

    /// 同じレーンに留まるとチャージが貯まり、レーンを変えるとリセットされる
    #[test]
    fn charge_builds_and_resets() {
        let mut w = World::new(stages::all().remove(0));
        for _ in 0..(60 * 4) {
            w.set_lane(1);
            w.step(1.0 / 60.0);
            w.events.clear();
        }
        assert!(w.charge >= 0.99, "charge {}", w.charge);
        for _ in 0..30 {
            w.set_lane(0);
            w.step(1.0 / 60.0);
            w.events.clear();
        }
        assert!(w.charge < 0.5, "charge after switch {}", w.charge);
    }
}
