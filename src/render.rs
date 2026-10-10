//! 描画と演出(トップダウン、画面下から上へ進む)。
//! 演出用の状態(パーティクル・揺れ・バナー)は `Fx` が持ち、sim の `GameEvent` から作る。

use macroquad::prelude::*;

use crate::sim::*;
use crate::stages::{lane_x, EnemyKind, GateOp, PowerUp, LANES, LANE_W};

// ───────────────────────── アセット ─────────────────────────

pub struct Gfx {
    grunt: [Texture2D; 3],
    brute: Texture2D,
    boss: [Texture2D; 2],
    soldier: Texture2D,
}

impl Gfx {
    pub fn load() -> Gfx {
        let tex = |bytes: &[u8]| {
            let t = Texture2D::from_file_with_format(bytes, None);
            t.set_filter(FilterMode::Linear);
            t
        };
        Gfx {
            grunt: [tex(include_bytes!("../assets/grunt1.png")), tex(include_bytes!("../assets/grunt2.png")), tex(include_bytes!("../assets/grunt3.png"))],
            brute: tex(include_bytes!("../assets/brute.png")),
            boss: [tex(include_bytes!("../assets/boss.png")), tex(include_bytes!("../assets/boss2.png"))],
            soldier: tex(include_bytes!("../assets/soldier.png")),
        }
    }
}

pub struct Ui {
    pub font: Option<Font>,
    /// 日本語フォントが読めたか
    pub jp: bool,
}

impl Ui {
    pub fn load() -> Ui {
        let candidates = [
            "C:/Windows/Fonts/YuGothB.ttc",
            "C:/Windows/Fonts/meiryob.ttc",
            "C:/Windows/Fonts/meiryo.ttc",
            "C:/Windows/Fonts/msgothic.ttc",
        ];
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(f) = load_ttf_font_from_bytes(&bytes) {
                    return Ui { font: Some(f), jp: true };
                }
            }
        }
        Ui { font: None, jp: false }
    }

    pub fn tr<'a>(&self, ja: &'a str, en: &'a str) -> &'a str {
        if self.jp { ja } else { en }
    }

    pub fn text(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        let params = TextParams { font: self.font.as_ref(), font_size: size as u16, color, ..Default::default() };
        let mut shadow = params.clone();
        shadow.color = Color::new(0.0, 0.0, 0.0, 0.6 * color.a);
        draw_text_ex(s, x + 2.0, y + 2.0, shadow);
        draw_text_ex(s, x, y, params);
    }

    pub fn text_center(&self, s: &str, cx: f32, y: f32, size: f32, color: Color) {
        let d = measure_text(s, self.font.as_ref(), size as u16, 1.0);
        self.text(s, cx - d.width / 2.0, y, size, color);
    }
}

// ───────────────────────── 演出状態 ─────────────────────────

struct Particle {
    x: f32,
    z: f32,
    vx: f32,
    vz: f32,
    ttl: f32,
    max: f32,
    size: f32,
    color: Color,
}

struct Ring {
    x: f32,
    z: f32,
    ttl: f32,
    max: f32,
    radius: f32,
    color: Color,
}

struct Popup {
    x: f32,
    z: f32,
    text: String,
    color: Color,
    size: f32,
    ttl: f32,
    max: f32,
}

pub struct Fx {
    particles: Vec<Particle>,
    rings: Vec<Ring>,
    popups: Vec<Popup>,
    shake: f32,
    /// 画面全体のフラッシュ(色, 残り)
    flash: (Color, f32),
    /// 中央バナー(文字, 色, 残り, 最大)
    banner: Option<(String, Color, f32, f32)>,
    /// 直近の一斉射からの時間(銃口の閃光)
    muzzle: f32,
    /// 損失の集計(0.3 秒ごとに 1 つの "-N" にまとめる)
    loss_acc: i32,
    loss_timer: f32,
    /// 武器 HUD のパルス [dmg, rate, pierce, spread]
    pulse: [f32; 4],
    units_pulse: f32,
    /// 敵に当たって減ったときに赤く光る
    hurt: f32,
    last_units: i32,
    just_pop: bool,
    seed: u32,
}

impl Fx {
    pub fn new() -> Fx {
        Fx {
            particles: Vec::new(),
            rings: Vec::new(),
            popups: Vec::new(),
            shake: 0.0,
            flash: (WHITE, 0.0),
            banner: None,
            muzzle: 0.0,
            loss_acc: 0,
            loss_timer: 0.0,
            pulse: [0.0; 4],
            units_pulse: 0.0,
            hurt: 0.0,
            last_units: 0,
            just_pop: false,
            seed: 12345,
        }
    }

    fn rnd(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    fn burst(&mut self, x: f32, z: f32, n: usize, speed: f32, size: f32, color: Color, life: f32) {
        for _ in 0..n {
            let a = self.rnd() * std::f32::consts::TAU;
            let s = speed * (0.3 + 0.7 * self.rnd());
            let ttl = life * (0.6 + 0.4 * self.rnd());
            let sz = size * (0.6 + 0.8 * self.rnd());
            self.particles.push(Particle { x, z, vx: a.cos() * s, vz: a.sin() * s, ttl, max: life, size: sz, color });
        }
    }

    fn ring(&mut self, x: f32, z: f32, radius: f32, color: Color, life: f32) {
        self.rings.push(Ring { x, z, ttl: life, max: life, radius, color });
    }

    fn popup(&mut self, x: f32, z: f32, text: String, color: Color, size: f32, life: f32) {
        self.popups.push(Popup { x, z, text, color, size, ttl: life, max: life });
    }

    fn banner(&mut self, text: String, color: Color, life: f32) {
        self.banner = Some((text, color, life, life));
    }

    pub fn reset(&mut self) {
        *self = Fx::new();
    }

    /// sim の出来事(drain 済み)を演出に変換
    pub fn handle_events(&mut self, events: Vec<GameEvent>, world: &World, jp: bool) {
        for ev in events {
            match ev {
                GameEvent::Volley => self.muzzle = 0.07,
                GameEvent::Kill { x, z, kind } => match kind {
                    EnemyKind::Grunt => {
                        self.burst(x, z, 6, 3.0, 4.0, Color::new(0.75, 0.25, 0.25, 1.0), 0.4);
                        self.burst(x, z, 3, 2.0, 3.0, Color::new(0.6, 0.85, 0.4, 1.0), 0.3);
                    }
                    EnemyKind::Brute => {
                        self.burst(x, z, 24, 6.0, 6.0, Color::new(0.8, 0.2, 0.2, 1.0), 0.7);
                        self.ring(x, z, 2.0, Color::new(1.0, 0.6, 0.3, 1.0), 0.5);
                        self.shake = self.shake.max(5.0);
                    }
                    EnemyKind::Boss => {
                        for i in 0..6 {
                            let ox = (i as f32 - 2.5) * 0.7;
                            self.burst(x + ox, z, 20, 7.0, 7.0, Color::new(1.0, 0.7, 0.25, 1.0), 1.0);
                        }
                        self.ring(x, z, 5.0, Color::new(1.0, 0.9, 0.5, 1.0), 0.9);
                        self.shake = 16.0;
                        self.flash = (Color::new(1.0, 0.95, 0.8, 1.0), 0.5);
                    }
                },
                GameEvent::UnitsLost { x, z, n } => {
                    self.loss_acc += n;
                    self.hurt = 0.35;
                    self.burst(x, z, 4, 3.0, 4.0, Color::new(1.0, 0.3, 0.25, 1.0), 0.35);
                    self.ring(x, z, 0.8, Color::new(1.0, 0.3, 0.25, 0.9), 0.25);
                    self.shake = self.shake.max(2.5);
                }
                GameEvent::CrateBroken { x, z, reward } => {
                    let c = power_color(reward);
                    self.burst(x, z, 26, 6.0, 5.0, c, 0.8);
                    self.ring(x, z, 2.5, c, 0.5);
                    self.ring(world.player_x, world.player_z, world.squad_radius() + 1.5, c, 0.7);
                    self.burst(world.player_x, world.player_z, 18, 5.0, 4.0, c, 0.7);
                    self.banner(reward.name(jp).to_string(), c, 1.4);
                    self.flash = (Color::new(c.r, c.g, c.b, 1.0), 0.25);
                    let i = match reward {
                        PowerUp::Damage => 0,
                        PowerUp::FireRate => 1,
                        PowerUp::Pierce => 2,
                        PowerUp::Spread => 3,
                    };
                    self.pulse[i] = 1.0;
                }
                GameEvent::ReinforceSpawn => {
                    self.banner((if jp { "救援物資が到着!" } else { "SUPPLIES ARRIVED!" }).to_string(), Color::new(0.4, 1.0, 0.55, 1.0), 1.8);
                }
                GameEvent::Reinforce { x, z, n } => {
                    let c = Color::new(0.4, 1.0, 0.55, 1.0);
                    self.burst(x, z, 28, 6.0, 5.0, c, 0.8);
                    self.ring(x, z, 2.5, c, 0.5);
                    self.ring(world.player_x, world.player_z, world.squad_radius() + 1.5, c, 0.7);
                    self.burst(world.player_x, world.player_z, 22, 5.0, 4.0, c, 0.7);
                    self.popup(world.player_x, world.player_z + 2.0, format!("+{n}"), c, 64.0, 1.4);
                    self.banner((if jp { "増援到着!" } else { "REINFORCEMENTS!" }).to_string(), c, 1.4);
                    self.flash = (c, 0.25);
                    self.units_pulse = 1.0;
                }
                GameEvent::Gate { x, z, op, delta } => {
                    let good = delta >= 0 && op.is_good();
                    let c = if good { Color::new(0.45, 0.8, 1.0, 1.0) } else { Color::new(1.0, 0.4, 0.4, 1.0) };
                    let txt = if delta >= 0 { format!("+{delta}") } else { format!("{delta}") };
                    self.popup(x, z + 1.0, txt, c, 56.0, 1.1);
                    self.ring(x, z, 2.0, c, 0.5);
                    self.burst(x, z, 20, 5.0, 4.0, c, 0.7);
                    self.units_pulse = 1.0;
                    if !good {
                        self.shake = self.shake.max(3.0);
                    }
                }
                GameEvent::BossIntro => {
                    self.banner(self.tr_boss(jp), Color::new(1.0, 0.35, 0.3, 1.0), 2.2);
                    self.shake = 8.0;
                }
                GameEvent::BossWindup => {}
                GameEvent::JustDodge => {
                    self.just_pop = true;
                    let c = Color::new(1.0, 0.85, 0.2, 1.0);
                    self.popup(world.player_x, world.player_z + 2.2, (if jp { "ジャスト回避!" } else { "JUST DODGE!" }).to_string(), c, 56.0, 1.3);
                    self.ring(world.player_x, world.player_z, world.squad_radius() + 1.8, c, 0.6);
                    self.burst(world.player_x, world.player_z, 26, 6.0, 5.0, c, 0.7);
                    self.flash = (c, 0.22);
                }
                GameEvent::BossSlam { hit, lost, danger } => {
                    self.shake = 14.0;
                    if let Some(b) = world.boss() {
                        for l in 0..LANES {
                            if danger[l] {
                                self.ring(lane_x(l), b.z - 3.0, 1.8, Color::new(1.0, 0.5, 0.2, 1.0), 0.5);
                                self.burst(lane_x(l), b.z - 3.0, 10, 5.0, 5.0, Color::new(0.8, 0.7, 0.6, 1.0), 0.6);
                            }
                        }
                    }
                    let just = std::mem::take(&mut self.just_pop);
                    if hit {
                        self.flash = (Color::new(1.0, 0.1, 0.1, 1.0), 0.35);
                        self.popup(world.player_x, world.player_z + 2.0, format!("-{lost}"), Color::new(1.0, 0.3, 0.3, 1.0), 72.0, 1.4);
                    } else if !just && danger.iter().filter(|&&d| d).count() >= 2 {
                        self.popup(world.player_x, world.player_z + 2.0, (if jp { "回避!" } else { "DODGE!" }).to_string(), Color::new(0.6, 1.0, 0.7, 1.0), 48.0, 1.0);
                    }
                }
            }
        }
        // 戦闘で減った数をまとめて表示
        if world.units != self.last_units {
            self.units_pulse = self.units_pulse.max(0.6);
            self.last_units = world.units;
        }
    }

    fn tr_boss(&self, jp: bool) -> String {
        (if jp { "ボス戦!" } else { "BOSS BATTLE!" }).to_string()
    }

    pub fn update(&mut self, dt: f32, world: &World) {
        for p in self.particles.iter_mut() {
            p.x += p.vx * dt;
            p.z += p.vz * dt;
            p.vx *= 0.96;
            p.vz *= 0.96;
            p.ttl -= dt;
        }
        self.particles.retain(|p| p.ttl > 0.0);
        for r in self.rings.iter_mut() {
            r.ttl -= dt;
        }
        self.rings.retain(|r| r.ttl > 0.0);
        for p in self.popups.iter_mut() {
            p.ttl -= dt;
            p.z += dt * 1.6;
        }
        self.popups.retain(|p| p.ttl > 0.0);
        self.shake = (self.shake - dt * 30.0).max(0.0);
        self.flash.1 = (self.flash.1 - dt).max(0.0);
        self.muzzle = (self.muzzle - dt).max(0.0);
        self.hurt = (self.hurt - dt).max(0.0);
        self.units_pulse = (self.units_pulse - dt * 3.0).max(0.0);
        for p in self.pulse.iter_mut() {
            *p = (*p - dt * 1.2).max(0.0);
        }
        if let Some(b) = self.banner.as_mut() {
            b.2 -= dt;
        }
        if matches!(self.banner, Some((_, _, t, _)) if t <= 0.0) {
            self.banner = None;
        }
        self.loss_timer += dt;
        if self.loss_timer > 0.3 {
            self.loss_timer = 0.0;
            if self.loss_acc > 0 {
                let n = self.loss_acc;
                self.loss_acc = 0;
                let (x, z) = (world.player_x, world.player_z + world.squad_radius() + 0.5);
                self.popup(x, z, format!("-{n}"), Color::new(1.0, 0.35, 0.35, 1.0), 40.0, 0.8);
            }
        }
    }
}

// ───────────────────────── 座標変換 ─────────────────────────

struct View {
    scale: f32,
    w: f32,
    base_y: f32,
    pz: f32,
    ox: f32,
    oy: f32,
}

impl View {
    fn new(world: &World, fx: &Fx) -> View {
        let w = screen_width();
        let t = get_time() as f32;
        let s = fx.shake;
        View {
            scale: w / 12.0,
            w,
            base_y: screen_height() * 0.84,
            pz: world.player_z,
            ox: (t * 61.0).sin() * s,
            oy: (t * 73.0).cos() * s,
        }
    }
    fn sx(&self, x: f32) -> f32 {
        self.w / 2.0 + x * self.scale + self.ox
    }
    fn sy(&self, z: f32) -> f32 {
        self.base_y - (z - self.pz) * self.scale + self.oy
    }
    /// 画面に映る z の範囲(余白付き)。上端まで最初から描くために使う
    fn z_range(&self) -> (f32, f32) {
        let h = screen_height();
        (self.pz - (h - self.base_y) / self.scale - 3.0, self.pz + self.base_y / self.scale + 3.0)
    }
}

fn power_color(p: PowerUp) -> Color {
    match p {
        PowerUp::Damage => Color::new(1.0, 0.45, 0.2, 1.0),
        PowerUp::FireRate => Color::new(1.0, 0.85, 0.2, 1.0),
        PowerUp::Pierce => Color::new(0.3, 0.9, 1.0, 1.0),
        PowerUp::Spread => Color::new(0.5, 1.0, 0.45, 1.0),
    }
}

fn gate_color(op: GateOp) -> Color {
    match op {
        GateOp::Add(a) if a < 0 => Color::new(0.95, 0.2, 0.25, 1.0),
        GateOp::Add(_) => Color::new(0.2, 0.55, 1.0, 1.0),
        GateOp::Mul(_) => Color::new(0.15, 0.8, 0.45, 1.0),
    }
}

/// ダメージ強化で弾の色が変わる
fn bullet_color(damage_lv: u32) -> Color {
    match damage_lv {
        0 => Color::new(0.75, 0.95, 1.0, 1.0),
        1 => Color::new(1.0, 0.95, 0.45, 1.0),
        2 => Color::new(1.0, 0.65, 0.2, 1.0),
        3 => Color::new(1.0, 0.35, 0.25, 1.0),
        _ => Color::new(0.95, 0.4, 1.0, 1.0),
    }
}

fn with_a(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

fn draw_sprite(tex: &Texture2D, cx: f32, bottom: f32, height: f32, tint: Color, rot: f32) {
    let w = height * tex.width() / tex.height();
    draw_texture_ex(
        tex,
        cx - w / 2.0,
        bottom - height,
        tint,
        DrawTextureParams { dest_size: Some(vec2(w, height)), rotation: rot, pivot: Some(vec2(cx, bottom)), ..Default::default() },
    );
}

// ───────────────────────── ワールド描画 ─────────────────────────

/// 画面に出す実行情報
pub struct HudInfo {
    pub hi_speed: bool,
    /// 実時間の経過(秒)
    pub time: f32,
}

pub fn draw_world(world: &World, ui: &Ui, gfx: &Gfx, fx: &Fx, info: &HudInfo) {
    let v = View::new(world, fx);
    let h = screen_height();
    let t = world.time;

    draw_background(world, &v, fx);

    // 地面の危険ゾーン(ボスのスラム予告)・ゲート・コンテナ
    draw_danger(world, &v);
    for g in &world.gates {
        draw_gate(g, ui, &v);
    }
    for c in &world.crates {
        draw_crate(c, ui, &v);
    }

    // 敵(奥から手前へ)
    let mut order: Vec<&Enemy> = world.enemies.iter().collect();
    order.sort_by(|a, b| b.z.partial_cmp(&a.z).unwrap());
    for e in order {
        draw_enemy(e, world, gfx, &v, t);
    }

    // 弾
    let wp = &world.weapon;
    let bc = bullet_color(wp.damage_lv);
    let size = 2.6 + 1.1 * wp.damage_lv as f32;
    let tracer = 0.3 + 0.55 * wp.pierce_lv as f32;
    for b in &world.bullets {
        let (x, y) = (v.sx(b.x), v.sy(b.z));
        let sp = (b.vx * b.vx + b.vz * b.vz).sqrt().max(0.01);
        let (dx, dz) = (b.vx / sp, b.vz / sp);
        let (tx, ty) = (v.sx(b.x - dx * tracer), v.sy(b.z - dz * tracer));
        draw_line(tx, ty, x, y, size * 1.6, with_a(bc, 0.35));
        draw_line(tx, ty, x, y, size, with_a(bc, 0.95));
        draw_circle(x, y, size * 0.9, WHITE);
    }

    draw_squad(world, gfx, &v, fx);

    // パーティクル・リング
    for p in &fx.particles {
        let k = (p.ttl / p.max).clamp(0.0, 1.0);
        draw_circle(v.sx(p.x), v.sy(p.z), p.size * (0.4 + 0.6 * k), with_a(p.color, k));
    }
    for r in &fx.rings {
        let k = 1.0 - (r.ttl / r.max).clamp(0.0, 1.0);
        draw_circle_lines(v.sx(r.x), v.sy(r.z), r.radius * v.scale * (0.3 + 0.9 * k), 4.0, with_a(r.color, 1.0 - k));
    }
    for p in &fx.popups {
        let k = (p.ttl / p.max).clamp(0.0, 1.0);
        let pop = 1.0 + 0.4 * (1.0 - k).min(0.2) * 5.0 * (if k > 0.8 { 1.0 } else { 0.0 });
        ui.text_center(&p.text, v.sx(p.x), v.sy(p.z), p.size * pop, with_a(p.color, (k * 2.0).min(1.0)));
    }

    // 被弾の赤いふち / 画面フラッシュ
    if fx.hurt > 0.0 {
        let a = fx.hurt / 0.35 * 0.35;
        let c = Color::new(0.9, 0.0, 0.0, a);
        let w = v.w;
        draw_rectangle(0.0, 0.0, w, 26.0, c);
        draw_rectangle(0.0, h - 26.0, w, 26.0, c);
        draw_rectangle(0.0, 0.0, 22.0, h, c);
        draw_rectangle(w - 22.0, 0.0, 22.0, h, c);
    }
    if fx.flash.1 > 0.0 {
        draw_rectangle(0.0, 0.0, v.w, h, with_a(fx.flash.0, fx.flash.1 * 0.5));
    }

    if info.hi_speed {
        draw_speed_lines(&v);
    }
    draw_hud(world, ui, &v, fx, info);
}

fn draw_background(world: &World, v: &View, _fx: &Fx) {
    let h = screen_height();
    let t = world.time;
    // 海(グラデーション)
    let bands = 14;
    for i in 0..bands {
        let k = i as f32 / bands as f32;
        let s = world.stage.theme.sea;
        let c = Color::new(s[0] + 0.05 * k, s[1] + 0.1 * k, s[2] + 0.1 * k, 1.0);
        draw_rectangle(0.0, h * k, v.w, h / bands as f32 + 1.0, c);
    }
    for i in 0..10 {
        let y = ((i as f32 * 97.0 + t * 20.0 + world.player_z * v.scale * 0.5) % (h + 60.0)) - 30.0;
        let x = (i as f32 * 173.0) % v.w;
        draw_rectangle(x, y, 60.0, 3.0, Color::new(1.0, 1.0, 1.0, 0.07));
    }

    // 橋
    let left = v.sx(-TRACK_HALF - 0.45);
    let width = (TRACK_HALF + 0.45) * 2.0 * v.scale;
    draw_rectangle(left - 4.0, 0.0, width + 8.0, h, Color::new(0.2, 0.22, 0.27, 1.0));
    for l in 0..LANES {
        let x0 = v.sx(lane_x(l) - LANE_W / 2.0);
        let rd = world.stage.theme.road;
        let dk = if l % 2 == 0 { 1.0 } else { 0.95 };
        draw_rectangle(x0, 0.0, LANE_W * v.scale + 1.0, h, Color::new(rd[0] * dk, rd[1] * dk, rd[2] * dk, 1.0));
    }
    // 床パネルの目地(流れる)
    let step = 3.0;
    let (zlo, zhi) = v.z_range();
    let first = (zlo / step).floor() as i32;
    let last = (zhi / step).ceil() as i32;
    for k in first..=last {
        let y = v.sy(k as f32 * step);
        if y > -4.0 && y < h + 4.0 {
            draw_rectangle(left, y, width, 2.0, Color::new(0.0, 0.0, 0.0, 0.10));
            draw_rectangle(left, y + 2.0, width, 1.0, Color::new(1.0, 1.0, 1.0, 0.06));
        }
    }
    // レーン境界の破線
    for l in 1..LANES {
        let x = v.sx(lane_x(l) - LANE_W / 2.0);
        let first = (zlo / 2.0).floor() as i32;
        let last = (zhi / 2.0).ceil() as i32;
        for k in first..=last {
            let y = v.sy(k as f32 * 2.0);
            draw_rectangle(x - 2.0, y - 0.5 * v.scale, 4.0, 0.9 * v.scale, Color::new(1.0, 1.0, 1.0, 0.55));
        }
    }
    // 欄干
    for side in [-1.0f32, 1.0] {
        let x = v.sx(side * (TRACK_HALF + 0.3));
        draw_rectangle(x - 7.0, 0.0, 14.0, h, Color::new(0.82, 0.85, 0.9, 1.0));
        draw_rectangle(x - 7.0, 0.0, 4.0, h, Color::new(1.0, 1.0, 1.0, 0.5));
        let first = (zlo / 4.0).floor() as i32;
        let last = (zhi / 4.0).ceil() as i32;
        for k in first..=last {
            let y = v.sy(k as f32 * 4.0);
            draw_rectangle(x - 11.0, y - 5.0, 22.0, 10.0, Color::new(0.55, 0.58, 0.65, 1.0));
        }
    }

    // ゴールライン
    let fy = v.sy(world.stage.finish_z);
    if fy > -20.0 && fy < h + 20.0 {
        let cells = 24;
        for i in 0..cells {
            for row in 0..2 {
                let c = if (i + row) % 2 == 0 { WHITE } else { BLACK };
                draw_rectangle(left + i as f32 * width / cells as f32, fy - 8.0 + row as f32 * 8.0, width / cells as f32 + 1.0, 8.0, c);
            }
        }
    }
}

fn draw_danger(world: &World, v: &View) {
    let Some(b) = world.boss() else { return };
    let ai = &world.boss_ai;
    if ai.strikes.is_empty() {
        return;
    }
    let pulse = ((get_time() as f32) * 16.0).sin() * 0.5 + 0.5;
    let y0 = v.sy(b.z - 1.5);
    let h = screen_height();
    // 着弾順の番号(未着弾のものだけ)
    let mut pending: Vec<usize> = (0..ai.strikes.len()).filter(|&i| !ai.strikes[i].done).collect();
    pending.sort_by(|&a, &b| ai.strikes[a].slam_at.partial_cmp(&ai.strikes[b].slam_at).unwrap());
    for (i, s) in ai.strikes.iter().enumerate() {
        let showing = if s.done { s.after > 0.0 } else { ai.clock >= s.warn_from };
        if !showing {
            continue;
        }
        let k = ((ai.clock - s.warn_from) / (s.slam_at - s.warn_from).max(0.01)).clamp(0.0, 1.0);
        let order = pending.iter().position(|&p| p == i);
        for l in 0..LANES {
            if !s.danger[l] {
                continue;
            }
            let x0 = v.sx(lane_x(l) - LANE_W / 2.0);
            let a = if s.done { 0.6 * s.after / 0.35 } else { 0.14 + 0.28 * k * (0.6 + 0.4 * pulse) };
            let col = if s.done { Color::new(1.0, 0.55, 0.15, a) } else { Color::new(1.0, 0.1, 0.1, a) };
            draw_rectangle(x0, y0, LANE_W * v.scale, h - y0, col);
            if !s.done {
                draw_rectangle_lines(x0 + 2.0, y0, LANE_W * v.scale - 4.0, h - y0, 4.0, Color::new(1.0, 0.3, 0.2, 0.3 + 0.6 * k * (0.5 + 0.5 * pulse)));
                // 注意マーク(複数あるときは着弾順の番号)
                let cy = v.base_y - 3.0 * v.scale;
                let cx = v.sx(lane_x(l));
                draw_poly(cx, cy, 3, 34.0, -90.0, Color::new(1.0, 0.85, 0.1, 0.5 + 0.5 * pulse));
                let label = match order {
                    Some(o) if pending.len() > 1 => format!("{}", o + 1),
                    _ => "!".to_string(),
                };
                draw_text(&label, cx - 6.0, cy + 12.0, 40.0, Color::new(0.2, 0.0, 0.0, 1.0));
                // 着弾までの縮むリング
                draw_circle_lines(cx, cy, 34.0 + 46.0 * (1.0 - k), 3.0, Color::new(1.0, 0.9, 0.3, 0.8 * k));
            }
        }
    }
}

fn draw_gate(g: &Gate, ui: &Ui, v: &View) {
    let y = v.sy(g.z);
    let h = screen_height();
    if y < -80.0 || y > h + 80.0 {
        return;
    }
    let th = 1.5 * v.scale;
    for l in 0..LANES {
        let op = g.ops[l];
        let mut c = gate_color(op);
        let taken = g.taken.map(|t| t == l);
        let alpha = match taken {
            None => 0.78,
            Some(true) => 0.45,
            Some(false) => 0.15,
        };
        let x0 = v.sx(lane_x(l) - LANE_W / 2.0) + 3.0;
        let w = LANE_W * v.scale - 6.0;
        c.a = alpha;
        draw_rectangle(x0, y - th / 2.0, w, th, c);
        draw_rectangle(x0, y - th / 2.0, w, th * 0.35, Color::new(1.0, 1.0, 1.0, 0.18 * alpha));
        draw_rectangle_lines(x0, y - th / 2.0, w, th, 4.0, Color::new(1.0, 1.0, 1.0, alpha));
        // 柱
        draw_rectangle(x0 - 3.0, y - th / 2.0 - 6.0, 8.0, th + 12.0, Color::new(0.85, 0.88, 0.95, 1.0));
        draw_rectangle(x0 + w - 5.0, y - th / 2.0 - 6.0, 8.0, th + 12.0, Color::new(0.85, 0.88, 0.95, 1.0));
        ui.text_center(&op.label(), x0 + w / 2.0, y + 18.0, 56.0, WHITE);
    }
}

fn draw_crate(c: &Crate, ui: &Ui, v: &View) {
    let y = v.sy(c.z);
    let h = screen_height();
    if y < -100.0 || y > h + 100.0 {
        return;
    }
    let w = 2.4 * v.scale;
    let hh = 1.6 * v.scale;
    let x = v.sx(c.x) - w / 2.0;
    let pc = power_color(c.reward);
    let flash = c.hit_timer > 0.0;
    let supply = c.reinforce > 0;
    let (body, accent) = if supply {
        (if flash { Color::new(0.8, 1.0, 0.85, 1.0) } else { Color::new(0.16, 0.5, 0.3, 1.0) }, Color::new(0.45, 1.0, 0.6, 1.0))
    } else {
        (if flash { Color::new(0.95, 0.8, 0.6, 1.0) } else { Color::new(0.5, 0.36, 0.17, 1.0) }, pc)
    };
    if supply {
        let p = 0.5 + 0.5 * (get_time() as f32 * 6.0).sin();
        draw_circle(v.sx(c.x), y, w * 0.75, Color::new(0.4, 1.0, 0.55, 0.12 + 0.12 * p));
    }
    draw_ellipse(v.sx(c.x), y + hh / 2.0, w * 0.6, 9.0, 0.0, Color::new(0.0, 0.0, 0.0, 0.3));
    draw_rectangle(x, y - hh / 2.0, w, hh, body);
    draw_rectangle(x, y - hh / 2.0, w, 14.0, accent);
    for i in 1..4 {
        draw_rectangle(x + i as f32 * w / 4.0 - 2.0, y - hh / 2.0 + 14.0, 4.0, hh - 14.0, Color::new(0.0, 0.0, 0.0, 0.2));
    }
    draw_rectangle_lines(x, y - hh / 2.0, w, hh, 4.0, if supply { Color::new(0.1, 0.35, 0.2, 1.0) } else { Color::new(0.25, 0.16, 0.06, 1.0) });
    ui.text_center(&format!("{}", c.hp.ceil() as i32), v.sx(c.x), y + 16.0, 44.0, WHITE);
    if supply {
        ui.text_center(&if ui.jp { format!("増援 +{}", c.reinforce) } else { format!("SUPPLY +{}", c.reinforce) }, v.sx(c.x), y - hh / 2.0 - 8.0, 26.0, accent);
    } else {
        ui.text_center(c.reward.short(ui.jp), v.sx(c.x), y - hh / 2.0 - 8.0, 26.0, pc);
    }
}

fn draw_enemy(e: &Enemy, world: &World, gfx: &Gfx, v: &View, t: f32) {
    let y = v.sy(e.z);
    let h = screen_height();
    let r = e.radius * v.scale;
    if y < -r * 3.0 - 40.0 || y > h + 80.0 {
        return;
    }
    let cx = v.sx(e.x);
    let hit = e.hit_timer > 0.0;
    let tint = if hit && e.kind != EnemyKind::Boss { Color::new(1.0, 0.6, 0.6, 1.0) } else { WHITE };
    let ph = t * 8.0 + e.id as f32 * 1.7;

    match e.kind {
        EnemyKind::Grunt => {
            let bob = ph.sin().abs() * 4.0;
            let bottom = y + r * 0.9 - bob;
            draw_ellipse(cx, y + r * 0.9, r * 1.1, r * 0.4, 0.0, Color::new(0.0, 0.0, 0.0, 0.3));
            let tex = &gfx.grunt[world.stage.grunt_tex[(e.id % 2) as usize]];
            let squash = if hit { 1.08 } else { 1.0 };
            draw_sprite(tex, cx, bottom, 1.45 * v.scale * squash, tint, (ph * 0.5).sin() * 0.12);
        }
        EnemyKind::Brute => {
            let bob = (ph * 0.6).sin().abs() * 6.0;
            draw_ellipse(cx, y + r * 0.8, r * 1.2, r * 0.4, 0.0, Color::new(0.0, 0.0, 0.0, 0.3));
            draw_sprite(&gfx.brute, cx, y + r * 0.8 - bob, 2.7 * v.scale, tint, (ph * 0.3).sin() * 0.05);
            let bw = r * 2.0;
            draw_rectangle(cx - bw / 2.0, y - r * 1.9, bw, 8.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_rectangle(cx - bw / 2.0, y - r * 1.9, bw * (e.hp / e.max_hp).clamp(0.0, 1.0), 8.0, RED);
        }
        EnemyKind::Boss => {
            let ai = &world.boss_ai;
            let mut lift = 0.0;
            let mut scale = 1.0;
            let mut col = tint;
            let pulse = (get_time() as f32 * 18.0).sin() * 0.5 + 0.5;
            match ai.phase {
                BossPhase::Attack if e.aggro && ai.warning() => {
                    let k = ai
                        .strikes
                        .iter()
                        .filter(|s| !s.done)
                        .map(|s| ((ai.clock - s.warn_from) / (s.slam_at - s.warn_from).max(0.01)).clamp(0.0, 1.0))
                        .fold(0.0, f32::max);
                    lift = k * r * 0.5;
                    scale = 1.0 + 0.08 * k;
                    col = Color::new(1.0, 1.0 - 0.45 * k * pulse, 1.0 - 0.55 * k * pulse, 1.0);
                }
                BossPhase::Recover => scale = 0.94,
                _ => {}
            }
            let bob = if e.aggro { (t * 2.5).sin() * 4.0 } else { 0.0 };
            draw_ellipse(cx, y + r * 0.95, r * 1.2, r * 0.38, 0.0, Color::new(0.0, 0.0, 0.0, 0.18));
            if hit {
                col = Color::new(1.0, 0.9, 0.9, 1.0);
            }
            draw_sprite(&gfx.boss[world.stage.boss_tex], cx, y + r * 0.95 - lift - bob, 5.4 * v.scale * scale, col, 0.0);
        }
    }
}

fn draw_squad(world: &World, gfx: &Gfx, v: &View, fx: &Fx) {
    let n = world.units.min(MAX_DRAWN_UNITS);
    let wp = &world.weapon;
    let golden = 2.399_963;
    let sr = world.squad_radius();
    let (px, pz) = (world.player_x, world.player_z);

    // 強化の総レベルに応じたオーラ(足元の光)
    let total = wp.damage_lv + wp.rate_lv + wp.pierce_lv + wp.spread_lv;
    let aura = bullet_color(wp.damage_lv);
    let cx = v.sx(px);
    let cy = v.sy(pz);
    draw_ellipse(cx, cy + 6.0, (sr + 0.5) * v.scale, (sr + 0.2) * v.scale * 0.8, 0.0, with_a(aura, 0.10 + 0.04 * total as f32));
    if total > 0 {
        draw_circle_lines(cx, cy, (sr + 0.3) * v.scale, 1.5, with_a(aura, 0.35));
    }

    // 同じレーンに留まるほど貯まるチャージ(円形ゲージ。満タンで連射が最大)
    let ring_r = (sr + 0.55) * v.scale;
    let charge = world.charge;
    draw_circle_lines(cx, cy, ring_r, 7.0, Color::new(0.0, 0.0, 0.0, 0.35));
    if charge > 0.0 {
        let full = charge >= 1.0;
        let pulse = if full { 0.7 + 0.3 * (get_time() as f32 * 12.0).sin() } else { 1.0 };
        let c = Color::new(0.35 + 0.65 * charge, 0.9 - 0.1 * charge, 1.0 - 0.8 * charge, pulse);
        draw_arc(cx, cy, ring_r, charge, 7.0, c);
    }
    // ジャスト回避の攻撃力アップ中
    if world.boost_t > 0.0 {
        let k = (world.boost_t / 0.5).clamp(0.0, 1.0);
        draw_circle_lines(cx, cy, ring_r + 10.0 + 14.0 * (1.0 - k), 5.0, Color::new(1.0, 0.85, 0.2, 0.9 * k));
        draw_ellipse(cx, cy + 6.0, (sr + 0.8) * v.scale, (sr + 0.5) * v.scale * 0.8, 0.0, Color::new(1.0, 0.85, 0.2, 0.25 * k));
    }

    // 拡散: 扇形のガイド
    if wp.streams() > 1 {
        let s = wp.streams() as i32;
        for i in 0..s {
            let lane = i as f32 - (s - 1) as f32 / 2.0;
            let a = lane * 0.10;
            let len = 4.5;
            draw_line(cx, cy, v.sx(px + a.sin() * len), v.sy(pz + a.cos() * len), 2.0, with_a(power_color(PowerUp::Spread), 0.18));
        }
    }

    // 兵士(奥から手前に)
    let mut pts: Vec<(f32, f32, f32)> = (0..n)
        .map(|i| {
            let rr = 0.11 * (i as f32).sqrt();
            let a = i as f32 * golden;
            (px + rr * a.cos(), pz + rr * a.sin(), i as f32)
        })
        .collect();
    pts.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let sh = if n <= 20 { 1.0 } else if n <= 60 { 0.82 } else { 0.7 } * v.scale;
    let muzzle_on = fx.muzzle > 0.0;
    let mcolor = bullet_color(wp.damage_lv);
    let msize = 5.0 + 3.0 * wp.rate_lv as f32;
    let recoil = if muzzle_on { 2.0 } else { 0.0 };
    for (k, (x, z, i)) in pts.iter().enumerate() {
        let bob = (world.time * 9.0 + i).sin().abs() * 2.5;
        let sx = v.sx(*x);
        let sy = v.sy(*z) - bob + recoil;
        draw_sprite(&gfx.soldier, sx, sy, sh * 0.78, WHITE, 0.0);
        // 銃口の閃光: 連射強化ほど大きく、弾色と同じ
        if muzzle_on && (k % 2 == 0 || n < 12) {
            let fy = sy - sh * 0.78 * 0.78;
            let fxx = sx + sh * 0.78 * 0.28;
            draw_poly(fxx, fy, 4, msize * 0.9, get_time() as f32 * 400.0, with_a(mcolor, 0.9));
            draw_circle(fxx, fy, msize * 0.4, WHITE);
        }
    }
}

fn draw_hud(world: &World, ui: &Ui, v: &View, fx: &Fx, info: &HudInfo) {
    let w = v.w;
    let h = screen_height();

    // 味方数(増減でパルス)
    let sy = v.sy(world.player_z - world.squad_radius() - 0.2) + 36.0;
    let size = 46.0 * (1.0 + 0.3 * fx.units_pulse);
    let col = if fx.hurt > 0.0 { Color::new(1.0, 0.5, 0.5, 1.0) } else { Color::new(0.75, 0.92, 1.0, 1.0) };
    ui.text_center(&format!("{}", world.units), v.sx(world.player_x), sy.min(h - 12.0), size, col);

    // 進行バー
    let progress = (world.player_z / world.stage.finish_z).clamp(0.0, 1.0);
    draw_rectangle(16.0, 14.0, w - 32.0, 12.0, Color::new(0.0, 0.0, 0.0, 0.5));
    draw_rectangle(16.0, 14.0, (w - 32.0) * progress, 12.0, Color::new(0.4, 0.85, 1.0, 1.0));
    draw_rectangle_lines(16.0, 14.0, w - 32.0, 12.0, 2.0, Color::new(1.0, 1.0, 1.0, 0.6));
    ui.text(world.stage.name, 16.0, 52.0, 26.0, WHITE);
    ui.text(&crate::ranking::format_time((info.time * 1000.0) as u32), w - 150.0, 52.0, 26.0, WHITE);
    if info.hi_speed {
        ui.text("HIGH SPEED", w - 190.0, 128.0, 24.0, Color::new(1.0, 0.6, 0.15, 1.0));
    }

    // ボス HP
    if let Some(b) = world.boss() {
        if b.aggro || b.z - world.player_z < 24.0 {
            let bw = w - 120.0;
            let x = 60.0;
            let y = 66.0;
            draw_rectangle(x, y, bw, 28.0, Color::new(0.0, 0.0, 0.0, 0.65));
            draw_rectangle(x, y, bw * (b.hp / b.max_hp).clamp(0.0, 1.0), 28.0, Color::new(0.9, 0.15, 0.15, 1.0));
            draw_rectangle(x, y, bw * (b.hp / b.max_hp).clamp(0.0, 1.0), 9.0, Color::new(1.0, 0.5, 0.45, 0.6));
            draw_rectangle_lines(x, y, bw, 28.0, 3.0, WHITE);
            ui.text_center(&format!("{}", b.hp.ceil() as i32), w / 2.0, y + 23.0, 28.0, WHITE);
        }
    }

    // 武器 HUD(取得でパルス)
    let wp = &world.weapon;
    let rows = [
        (PowerUp::Damage, wp.damage_lv),
        (PowerUp::FireRate, wp.rate_lv),
        (PowerUp::Pierce, wp.pierce_lv),
        (PowerUp::Spread, wp.spread_lv),
    ];
    for (i, (p, lv)) in rows.iter().enumerate() {
        let y = h - 108.0 + i as f32 * 26.0;
        let pulse = fx.pulse[i];
        let c = power_color(*p);
        let bw = 110.0 + 30.0 * pulse;
        draw_rectangle(10.0, y - 20.0, bw, 24.0, Color::new(0.0, 0.0, 0.0, 0.45 + 0.3 * pulse));
        draw_rectangle(10.0, y - 20.0, 5.0, 24.0, c);
        ui.text(&format!("{} Lv{}", p.short(ui.jp), lv), 22.0, y - 1.0, 21.0 + 5.0 * pulse, c);
    }

    // 中央バナー
    if let Some((text, c, ttl, max)) = &fx.banner {
        let k = (ttl / max).clamp(0.0, 1.0);
        let pop = 1.0 + 0.5 * (1.0 - k).min(0.15) * 6.0 * if k > 0.85 { 1.0 } else { 0.0 };
        let a = (k * 3.0).min(1.0);
        draw_rectangle(0.0, h * 0.30, w, 96.0, Color::new(0.0, 0.0, 0.0, 0.45 * a));
        ui.text_center(text, w / 2.0, h * 0.30 + 66.0, 62.0 * pop, with_a(*c, a));
    }

    // ボス戦: 回避のヒント
    if world.boss_ai.phase == BossPhase::Attack && world.boss_ai.warning() {
        ui.text_center(ui.tr("赤いレーンから逃げろ!", "Leave the red lanes!"), w / 2.0, h * 0.5, 34.0, Color::new(1.0, 0.9, 0.3, 1.0));
    }
}


/// ハイスピード時の流れる速度線
fn draw_speed_lines(v: &View) {
    let h = screen_height();
    let t = get_time() as f32;
    for i in 0..18 {
        let x = ((i as f32 * 211.7) % v.w).abs();
        let y = ((t * 900.0 + i as f32 * 137.0) % (h + 200.0)) - 100.0;
        draw_line(x, y, x, y + 70.0 + (i % 3) as f32 * 20.0, 2.0, Color::new(1.0, 1.0, 1.0, 0.10));
    }
}

// ───────────────────────── 画面(タイトル/結果/ランキング) ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    Retry,
    Next,
    Title,
    StartNormal,
    StartHi,
    Ranking,
    StagePrev,
    StageNext,
    ToggleMode,
    Save,
    Skip,
}

pub struct ButtonSpec {
    pub btn: Btn,
    pub rect: Rect,
    pub label: &'static str,
    pub label_en: &'static str,
    pub color: Color,
}

fn spec(btn: Btn, x: f32, y: f32, w: f32, h: f32, label: &'static str, label_en: &'static str, color: Color) -> ButtonSpec {
    ButtonSpec { btn, rect: Rect::new(x, y, w, h), label, label_en, color }
}

fn col_blue() -> Color {
    Color::new(0.2, 0.5, 0.95, 1.0)
}
fn col_orange() -> Color {
    Color::new(0.95, 0.5, 0.1, 1.0)
}
fn col_gray() -> Color {
    Color::new(0.5, 0.5, 0.58, 1.0)
}
fn col_green() -> Color {
    Color::new(0.2, 0.7, 0.35, 1.0)
}

fn column(top: f32, rows: &[(Btn, &'static str, &'static str, Color)]) -> Vec<ButtonSpec> {
    let w = screen_width();
    let bw = (w * 0.62).min(340.0);
    let bh = 60.0;
    let x = (w - bw) / 2.0;
    rows.iter().enumerate().map(|(i, (b, l, e, c))| spec(*b, x, top + i as f32 * (bh + 12.0), bw, bh, l, e, *c)).collect()
}

/// ボタンの配置(描画とクリック判定で同じものを使う)
pub fn result_buttons(has_next: bool, won: bool) -> Vec<ButtonSpec> {
    let mut rows: Vec<(Btn, &'static str, &'static str, Color)> = Vec::new();
    if has_next {
        rows.push((Btn::Next, "次のステージへ (N)", "Next Stage (N)", col_green()));
    }
    rows.push((Btn::Retry, "もう一度 (R)", "Retry (R)", col_blue()));
    if won {
        rows.push((Btn::Ranking, "ランキング (K)", "Ranking (K)", col_orange()));
    }
    rows.push((Btn::Title, "タイトルへ戻る (T)", "Back to Title (T)", col_gray()));
    column(screen_height() * 0.50, &rows)
}

pub fn title_buttons() -> Vec<ButtonSpec> {
    let (w, h) = (screen_width(), screen_height());
    let mut v = column(
        h * 0.64,
        &[
            (Btn::StartNormal, "ノーマル (Enter)", "NORMAL (Enter)", col_blue()),
            (Btn::StartHi, "ハイスピード (H)", "HIGH SPEED (H)", col_orange()),
            (Btn::Ranking, "ランキング (K)", "RANKING (K)", col_gray()),
        ],
    );
    v.push(spec(Btn::StagePrev, w * 0.12, h * 0.215, 54.0, 54.0, "<", "<", col_gray()));
    v.push(spec(Btn::StageNext, w * 0.88 - 54.0, h * 0.215, 54.0, 54.0, ">", ">", col_gray()));
    v
}

pub fn ranking_buttons() -> Vec<ButtonSpec> {
    let (w, h) = (screen_width(), screen_height());
    vec![
        spec(Btn::StagePrev, w * 0.08, h * 0.2, 54.0, 54.0, "<", "<", col_gray()),
        spec(Btn::StageNext, w * 0.92 - 54.0, h * 0.2, 54.0, 54.0, ">", ">", col_gray()),
        spec(Btn::ToggleMode, w * 0.5 - 150.0, h * 0.285, 300.0, 46.0, "モード切替 (Tab)", "Switch Mode (Tab)", col_orange()),
        spec(Btn::Title, w * 0.5 - 150.0, h * 0.89, 300.0, 56.0, "戻る (Esc)", "Back (Esc)", col_blue()),
    ]
}

pub fn name_entry_buttons() -> Vec<ButtonSpec> {
    let (w, h) = (screen_width(), screen_height());
    vec![
        spec(Btn::Save, w * 0.5 - 150.0, h * 0.62, 300.0, 60.0, "保存 (Enter)", "Save (Enter)", col_green()),
        spec(Btn::Skip, w * 0.5 - 150.0, h * 0.62 + 72.0, 300.0, 56.0, "保存しない (Esc)", "Skip (Esc)", col_gray()),
    ]
}

pub fn hit_button(specs: &[ButtonSpec], x: f32, y: f32) -> Option<Btn> {
    specs.iter().find(|b| b.rect.contains(vec2(x, y))).map(|b| b.btn)
}

fn draw_buttons(ui: &Ui, specs: &[ButtonSpec]) {
    let (mx, my) = mouse_position();
    for b in specs {
        let hover = b.rect.contains(vec2(mx, my));
        let r = b.rect;
        let k = if hover { 1.15 } else { 1.0 };
        let c = Color::new((b.color.r * k).min(1.0), (b.color.g * k).min(1.0), (b.color.b * k).min(1.0), 1.0);
        draw_rectangle(r.x + 3.0, r.y + 5.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.4));
        draw_rectangle(r.x, r.y, r.w, r.h, c);
        draw_rectangle(r.x, r.y, r.w, r.h * 0.45, Color::new(1.0, 1.0, 1.0, 0.15));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 3.0, if hover { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.6) });
        let size = if r.h < 50.0 { 26.0 } else { 30.0 };
        ui.text_center(ui.tr(b.label, b.label_en), r.x + r.w / 2.0, r.y + r.h / 2.0 + size * 0.33, size, WHITE);
    }
}

fn menu_background() {
    let (w, h) = (screen_width(), screen_height());
    for i in 0..20 {
        let k = i as f32 / 20.0;
        draw_rectangle(0.0, h * k, w, h / 20.0 + 1.0, Color::new(0.04 + 0.05 * k, 0.09 + 0.07 * k, 0.18 + 0.1 * k, 1.0));
    }
}

pub fn draw_title(ui: &Ui, gfx: &Gfx, stage: &crate::stages::Stage, stage_count: usize, stage_idx: usize) {
    let (w, h) = (screen_width(), screen_height());
    let t = get_time() as f32;
    menu_background();

    // ロゴ
    ui.text_center("GROWTH GUNNER", w / 2.0, h * 0.115, 50.0, Color::new(0.65, 0.92, 1.0, 1.0));
    ui.text_center(ui.tr("ゲートで増えて、撃って、育てろ!", "Multiply. Shoot. Grow."), w / 2.0, h * 0.115 + 32.0, 22.0, Color::new(1.0, 0.9, 0.5, 1.0));

    // ステージ選択
    ui.text_center(stage.name, w / 2.0, h * 0.215 + 38.0, 40.0, WHITE);
    if stage_count > 1 {
        ui.text_center(&format!("{}/{}", stage_idx + 1, stage_count), w / 2.0, h * 0.215 + 62.0, 18.0, Color::new(0.8, 0.85, 1.0, 1.0));
    }

    // 主役たち
    draw_sprite(&gfx.boss[stage.boss_tex], w * 0.5, h * 0.57 + (t * 2.0).sin() * 4.0, h * 0.25, Color::new(1.0, 1.0, 1.0, 0.95), 0.0);
    draw_sprite(&gfx.brute, w * 0.16, h * 0.585, h * 0.12, WHITE, (t * 2.0).sin() * 0.04);
    draw_sprite(&gfx.grunt[stage.grunt_tex[0]], w * 0.84, h * 0.585, h * 0.1, WHITE, (t * 6.0).sin() * 0.1);
    draw_sprite(&gfx.grunt[stage.grunt_tex[1]], w * 0.72, h * 0.59, h * 0.09, WHITE, (t * 6.0 + 1.0).sin() * 0.1);
    for i in 0..5 {
        draw_sprite(&gfx.soldier, w * (0.30 + 0.1 * i as f32), h * 0.625 - (i % 2) as f32 * 10.0, h * 0.085, WHITE, 0.0);
    }

    draw_buttons(ui, &title_buttons());
    ui.text_center(
        ui.tr("移動: ← → / A D(押した分だけ) / マウス・ドラッグ", "Move: Left/Right, A/D (hold) or mouse drag"),
        w / 2.0,
        h * 0.955,
        18.0,
        Color::new(0.85, 0.9, 1.0, 1.0),
    );
}

pub fn draw_ranking(ui: &Ui, ranking: &crate::ranking::Ranking, stage_name: &str, stage_idx: usize, hi: bool) {
    let (w, h) = (screen_width(), screen_height());
    menu_background();
    ui.text_center(ui.tr("ランキング", "RANKING"), w / 2.0, h * 0.11, 52.0, Color::new(1.0, 0.9, 0.4, 1.0));
    ui.text_center(stage_name, w / 2.0, h * 0.2 + 38.0, 38.0, WHITE);
    let (mode, mc) = if hi {
        (ui.tr("ハイスピード", "HIGH SPEED"), Color::new(1.0, 0.6, 0.15, 1.0))
    } else {
        (ui.tr("ノーマル", "NORMAL"), Color::new(0.6, 0.85, 1.0, 1.0))
    };
    ui.text_center(&format!("[ {mode} ]"), w / 2.0, h * 0.285 + 78.0, 26.0, mc);

    let top = ranking.top(stage_idx, hi);
    let y0 = h * 0.285 + 118.0;
    for i in 0..crate::ranking::MAX_ENTRIES {
        let y = y0 + i as f32 * 40.0;
        draw_rectangle(w * 0.1, y - 28.0, w * 0.8, 36.0, Color::new(1.0, 1.0, 1.0, if i % 2 == 0 { 0.06 } else { 0.02 }));
        let medal = match i {
            0 => Color::new(1.0, 0.85, 0.2, 1.0),
            1 => Color::new(0.8, 0.85, 0.9, 1.0),
            2 => Color::new(0.85, 0.55, 0.3, 1.0),
            _ => WHITE,
        };
        ui.text(&format!("{}", i + 1), w * 0.13, y, 26.0, medal);
        match top.get(i) {
            Some(e) => {
                ui.text(&e.name, w * 0.23, y, 26.0, WHITE);
                ui.text(&crate::ranking::format_time(e.ms), w * 0.64, y, 26.0, medal);
            }
            None => ui.text("---", w * 0.23, y, 26.0, Color::new(1.0, 1.0, 1.0, 0.3)),
        }
    }
    draw_buttons(ui, &ranking_buttons());
}

pub fn draw_name_entry(ui: &Ui, name: &str, ms: u32, rank: usize, hi: bool) {
    let (w, h) = (screen_width(), screen_height());
    menu_background();
    ui.text_center(ui.tr("ステージクリア!", "STAGE CLEAR!"), w / 2.0, h * 0.16, 56.0, Color::new(1.0, 0.9, 0.3, 1.0));
    ui.text_center(&crate::ranking::format_time(ms), w / 2.0, h * 0.16 + 70.0, 64.0, WHITE);
    let mode = if hi { ui.tr("ハイスピード", "HIGH SPEED") } else { ui.tr("ノーマル", "NORMAL") };
    let rank_text = if ui.jp { format!("{rank}位にランクイン!") } else { format!("Rank #{rank}!") };
    ui.text_center(&format!("{mode}  -  {rank_text}"), w / 2.0, h * 0.16 + 112.0, 28.0, Color::new(1.0, 0.7, 0.3, 1.0));

    ui.text_center(ui.tr("名前を入力 (半角英数字)", "Enter your name"), w / 2.0, h * 0.46, 24.0, Color::new(0.85, 0.9, 1.0, 1.0));
    let bw = 340.0;
    let bx = (w - bw) / 2.0;
    draw_rectangle(bx, h * 0.48, bw, 56.0, Color::new(0.0, 0.0, 0.0, 0.5));
    draw_rectangle_lines(bx, h * 0.48, bw, 56.0, 3.0, Color::new(0.5, 0.85, 1.0, 1.0));
    let caret = if (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
    ui.text_center(&format!("{name}{caret}"), w / 2.0, h * 0.48 + 40.0, 36.0, WHITE);
    draw_buttons(ui, &name_entry_buttons());
}

pub fn draw_result(ui: &Ui, world: &World, has_next: bool, clear: Option<(u32, Option<usize>)>, hi: bool) {
    let (w, h) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.62));
    let won = world.state == State::Won;
    let (title, col) = if won {
        (ui.tr("ステージクリア!", "STAGE CLEAR!"), Color::new(1.0, 0.9, 0.3, 1.0))
    } else {
        (ui.tr("全滅...", "DEFEATED"), Color::new(1.0, 0.4, 0.4, 1.0))
    };
    ui.text_center(title, w / 2.0, h * 0.2, 64.0, col);
    let stats = if ui.jp {
        format!("生存 {}  /  撃破 {}  /  損失 {}", world.units, world.kills, world.units_lost)
    } else {
        format!("Alive {}  /  Kills {}  /  Lost {}", world.units, world.kills, world.units_lost)
    };
    ui.text_center(&stats, w / 2.0, h * 0.2 + 46.0, 24.0, WHITE);
    if let Some((ms, rank)) = clear {
        let mode = if hi { ui.tr("ハイスピード", "HIGH SPEED") } else { ui.tr("ノーマル", "NORMAL") };
        ui.text_center(&format!("{}  {}", ui.tr("クリアタイム", "TIME"), crate::ranking::format_time(ms)), w / 2.0, h * 0.2 + 100.0, 38.0, WHITE);
        let line = match rank {
            Some(r) if ui.jp => format!("{mode}  {r}位に登録!"),
            Some(r) => format!("{mode}  Rank #{r} saved!"),
            None => format!("{mode}  {}", ui.tr("ランキング未登録", "Not registered")),
        };
        ui.text_center(&line, w / 2.0, h * 0.2 + 138.0, 24.0, Color::new(1.0, 0.7, 0.3, 1.0));
    }
    draw_buttons(ui, &result_buttons(has_next, won));
}

/// 真上から時計回りに `frac`(0..1)だけ描く円弧
fn draw_arc(cx: f32, cy: f32, r: f32, frac: f32, thick: f32, color: Color) {
    let segs = ((frac * 48.0).ceil() as i32).max(1);
    let total = frac.clamp(0.0, 1.0) * std::f32::consts::TAU;
    let start = -std::f32::consts::FRAC_PI_2;
    let mut prev = (cx + start.cos() * r, cy + start.sin() * r);
    for i in 1..=segs {
        let a = start + total * i as f32 / segs as f32;
        let p = (cx + a.cos() * r, cy + a.sin() * r);
        draw_line(prev.0, prev.1, p.0, p.1, thick, color);
        prev = p;
    }
}
