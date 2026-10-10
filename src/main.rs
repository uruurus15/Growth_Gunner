mod ranking;
mod render;
mod sim;
mod stages;

use macroquad::prelude::*;

use ranking::Ranking;
use render::{Btn, Fx, Gfx, HudInfo, Ui};
use sim::{bot, State, World};

/// ハイスピードモードの速度倍率(敵・弾・ボス・移動、すべてに掛かる)
const HI_SPEED: f32 = 2.0;
const DT: f32 = 1.0 / 60.0;

fn conf() -> Conf {
    Conf {
        window_title: "Growth Gunner".to_owned(),
        window_width: 540,
        window_height: 900,
        sample_count: 4,
        ..Default::default()
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Title,
    Playing,
    NameEntry,
    Result,
    Ranking,
}

/// デバッグ用の起動オプション: --auto --start --jump <z> --units <n> --lv <n> --hi --stage <n> --screen ranking
struct DebugOpts {
    auto: bool,
    start: bool,
    jump: Option<f32>,
    units: i32,
    lv: u32,
    hi: bool,
    stage: usize,
    ranking: bool,
}

fn parse_args() -> DebugOpts {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let has = |k: &str| args.iter().any(|a| a == k);
    DebugOpts {
        auto: has("--auto"),
        start: has("--start") || has("--auto"),
        jump: get("--jump").and_then(|s| s.parse().ok()),
        units: get("--units").and_then(|s| s.parse().ok()).unwrap_or(120),
        lv: get("--lv").and_then(|s| s.parse().ok()).unwrap_or(2),
        hi: has("--hi"),
        stage: get("--stage").and_then(|s| s.parse::<usize>().ok()).map(|n| n.saturating_sub(1)).unwrap_or(0),
        ranking: get("--screen").as_deref() == Some("ranking"),
    }
}

struct Game {
    opts: DebugOpts,
    stage_idx: usize,
    stage_count: usize,
    hi: bool,
    world: World,
    fx: Fx,
    ranking: Ranking,
    screen: Screen,
    accum: f32,
    /// ランキング用の実時間(秒)
    real_time: f32,
    /// 画面遷移に使ったクリックでキャラが動かないよう、離すまでマウス操作を無効にする
    mouse_armed: bool,
    name_buf: String,
    /// 直近のクリア: (ミリ秒, 登録順位)
    clear: Option<(u32, Option<usize>)>,
    pending_rank: usize,
    rank_stage: usize,
    rank_hi: bool,
}

impl Game {
    fn new_world(&self) -> World {
        let mut w = World::new(stages::all()[self.stage_idx].clone());
        if let Some(z) = self.opts.jump {
            w.debug_jump(z, self.opts.units, self.opts.lv);
        }
        w
    }

    fn start_run(&mut self, hi: bool) {
        self.hi = hi;
        self.world = self.new_world();
        self.fx.reset();
        self.accum = 0.0;
        self.real_time = 0.0;
        self.mouse_armed = false;
        self.clear = None;
        self.screen = Screen::Playing;
    }

    fn go_title(&mut self) {
        self.world = self.new_world();
        self.fx.reset();
        self.mouse_armed = false;
        self.screen = Screen::Title;
    }

    fn open_ranking(&mut self, stage: usize, hi: bool) {
        self.rank_stage = stage;
        self.rank_hi = hi;
        self.mouse_armed = false;
        self.screen = Screen::Ranking;
    }

    fn change_stage(&mut self, dir: i32) {
        let n = self.stage_count as i32;
        self.stage_idx = ((self.stage_idx as i32 + dir).rem_euclid(n)) as usize;
        self.world = self.new_world();
    }

    fn change_rank_stage(&mut self, dir: i32) {
        let n = self.stage_count as i32;
        self.rank_stage = ((self.rank_stage as i32 + dir).rem_euclid(n)) as usize;
    }

    fn speed(&self) -> f32 {
        if self.hi { HI_SPEED } else { 1.0 }
    }

    /// クリア直後: ランクインなら名前入力へ、そうでなければ結果画面へ
    fn on_won(&mut self) {
        let ms = (self.real_time * 1000.0) as u32;
        // 自動操縦のデバッグ実行は登録しない
        match self.ranking.rank_of(self.stage_idx, self.hi, ms) {
            Some(rank) if !self.opts.auto => {
                self.name_buf = self.ranking.last_name.clone();
                self.pending_rank = rank;
                self.clear = Some((ms, None));
                while get_char_pressed().is_some() {}
                self.screen = Screen::NameEntry;
            }
            _ => {
                self.clear = Some((ms, None));
                self.screen = Screen::Result;
            }
        }
    }
}

#[macroquad::main(conf)]
async fn main() {
    let opts = parse_args();
    let ui = Ui::load();
    let gfx = Gfx::load();
    let stage_count = stages::all().len();
    let stage_idx = opts.stage.min(stage_count - 1);
    let world = {
        let mut w = World::new(stages::all()[stage_idx].clone());
        if let Some(z) = opts.jump {
            w.debug_jump(z, opts.units, opts.lv);
        }
        w
    };
    let mut g = Game {
        stage_idx,
        stage_count,
        hi: opts.hi,
        world,
        fx: Fx::new(),
        ranking: Ranking::load(),
        screen: Screen::Title,
        accum: 0.0,
        real_time: 0.0,
        mouse_armed: false,
        name_buf: String::new(),
        clear: None,
        pending_rank: 1,
        rank_stage: stage_idx,
        rank_hi: false,
        opts,
    };
    if g.opts.start {
        let hi = g.opts.hi;
        g.start_run(hi);
    } else if g.opts.ranking {
        g.open_ranking(stage_idx, false);
    }

    loop {
        let frame = get_frame_time().min(0.1);
        let mouse_down = is_mouse_button_down(MouseButton::Left);
        if !mouse_down {
            g.mouse_armed = true;
        }
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let (mx, my) = mouse_position();
        let pressed = |k: KeyCode| is_key_pressed(k);

        match g.screen {
            Screen::Title => {
                if pressed(KeyCode::Escape) {
                    break;
                }
                let mut action = if clicked { render::hit_button(&render::title_buttons(), mx, my) } else { None };
                if (pressed(KeyCode::Enter) || pressed(KeyCode::KpEnter)) || pressed(KeyCode::Space) || pressed(KeyCode::Key1) {
                    action = Some(Btn::StartNormal);
                } else if pressed(KeyCode::H) || pressed(KeyCode::Key2) {
                    action = Some(Btn::StartHi);
                } else if pressed(KeyCode::K) {
                    action = Some(Btn::Ranking);
                } else if pressed(KeyCode::Left) || pressed(KeyCode::A) {
                    action = Some(Btn::StagePrev);
                } else if pressed(KeyCode::Right) || pressed(KeyCode::D) {
                    action = Some(Btn::StageNext);
                }
                match action {
                    Some(Btn::StartNormal) => g.start_run(false),
                    Some(Btn::StartHi) => g.start_run(true),
                    Some(Btn::Ranking) => g.open_ranking(g.stage_idx, false),
                    Some(Btn::StagePrev) => g.change_stage(-1),
                    Some(Btn::StageNext) => g.change_stage(1),
                    _ => {}
                }
            }
            Screen::Playing => {
                if pressed(KeyCode::Escape) {
                    g.go_title();
                } else {
                    let speed = g.speed();
                    // 入力: 押している間だけ動く。マウスは指した位置へ
                    let mut dir = 0.0;
                    if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
                        dir -= 1.0;
                    }
                    if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
                        dir += 1.0;
                    }
                    if dir != 0.0 {
                        g.world.steer(dir, frame * speed);
                    }
                    if g.mouse_armed && mouse_down {
                        let scale = screen_width() / 12.0;
                        g.world.set_target_x((mx - screen_width() / 2.0) / scale);
                    }
                    if g.opts.auto {
                        g.world.set_lane(bot::smart(&g.world));
                    }

                    g.real_time += frame;
                    g.accum += frame * speed;
                    while g.accum >= DT {
                        g.world.step(DT);
                        g.accum -= DT;
                    }
                    let events = std::mem::take(&mut g.world.events);
                    g.fx.handle_events(events, &g.world, ui.jp);
                    g.fx.update(frame, &g.world);
                    match g.world.state {
                        State::Playing => {}
                        State::Won => {
                            g.mouse_armed = false;
                            g.on_won();
                        }
                        State::Lost => {
                            g.mouse_armed = false;
                            g.screen = Screen::Result;
                        }
                    }
                }
            }
            Screen::NameEntry => {
                while let Some(c) = get_char_pressed() {
                    if !c.is_control() && g.name_buf.chars().count() < ranking::NAME_MAX {
                        g.name_buf.push(c);
                    }
                }
                if pressed(KeyCode::Backspace) {
                    g.name_buf.pop();
                }
                let mut action = if clicked { render::hit_button(&render::name_entry_buttons(), mx, my) } else { None };
                if (pressed(KeyCode::Enter) || pressed(KeyCode::KpEnter)) {
                    action = Some(Btn::Save);
                } else if pressed(KeyCode::Escape) {
                    action = Some(Btn::Skip);
                }
                match action {
                    Some(Btn::Save) => {
                        if let Some((ms, _)) = g.clear {
                            g.ranking.add(g.stage_idx, g.hi, ms, &g.name_buf);
                            g.clear = Some((ms, Some(g.pending_rank)));
                        }
                        g.screen = Screen::Result;
                    }
                    Some(Btn::Skip) => g.screen = Screen::Result,
                    _ => {}
                }
            }
            Screen::Result => {
                g.fx.update(frame, &g.world);
                let won = g.world.state == State::Won;
                let has_next = won && g.stage_idx + 1 < g.stage_count;
                let mut action = if clicked { render::hit_button(&render::result_buttons(has_next, won), mx, my) } else { None };
                if pressed(KeyCode::R) {
                    action = Some(Btn::Retry);
                } else if pressed(KeyCode::T) || pressed(KeyCode::Escape) {
                    action = Some(Btn::Title);
                } else if pressed(KeyCode::N) && has_next {
                    action = Some(Btn::Next);
                } else if pressed(KeyCode::K) && won {
                    action = Some(Btn::Ranking);
                }
                match action {
                    Some(Btn::Retry) => {
                        let hi = g.hi;
                        g.start_run(hi);
                    }
                    Some(Btn::Next) => {
                        g.stage_idx += 1;
                        let hi = g.hi;
                        g.start_run(hi);
                    }
                    Some(Btn::Title) => g.go_title(),
                    Some(Btn::Ranking) => g.open_ranking(g.stage_idx, g.hi),
                    _ => {}
                }
            }
            Screen::Ranking => {
                let mut action = if clicked { render::hit_button(&render::ranking_buttons(), mx, my) } else { None };
                if pressed(KeyCode::Escape) || (pressed(KeyCode::Enter) || pressed(KeyCode::KpEnter)) || pressed(KeyCode::Backspace) {
                    action = Some(Btn::Title);
                } else if pressed(KeyCode::Tab) || pressed(KeyCode::H) {
                    action = Some(Btn::ToggleMode);
                } else if pressed(KeyCode::Left) || pressed(KeyCode::A) {
                    action = Some(Btn::StagePrev);
                } else if pressed(KeyCode::Right) || pressed(KeyCode::D) {
                    action = Some(Btn::StageNext);
                }
                match action {
                    Some(Btn::Title) => g.go_title(),
                    Some(Btn::ToggleMode) => g.rank_hi = !g.rank_hi,
                    Some(Btn::StagePrev) => g.change_rank_stage(-1),
                    Some(Btn::StageNext) => g.change_rank_stage(1),
                    _ => {}
                }
            }
        }

        // 描画
        let info = HudInfo { hi_speed: g.hi, time: g.real_time };
        match g.screen {
            Screen::Title => {
                render::draw_title(&ui, &gfx, &g.world.stage, g.stage_count, g.stage_idx);
            }
            Screen::Ranking => {
                let name = stages::all()[g.rank_stage].name;
                render::draw_ranking(&ui, &g.ranking, name, g.rank_stage, g.rank_hi);
            }
            Screen::NameEntry => {
                let ms = g.clear.map(|c| c.0).unwrap_or(0);
                render::draw_name_entry(&ui, &g.name_buf, ms, g.pending_rank, g.hi);
            }
            Screen::Playing => render::draw_world(&g.world, &ui, &gfx, &g.fx, &info),
            Screen::Result => {
                render::draw_world(&g.world, &ui, &gfx, &g.fx, &info);
                let won = g.world.state == State::Won;
                render::draw_result(&ui, &g.world, won && g.stage_idx + 1 < g.stage_count, g.clear, g.hi);
            }
        }
        next_frame().await;
    }
}
