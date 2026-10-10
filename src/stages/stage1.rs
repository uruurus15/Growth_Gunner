use super::*;

const L: usize = 0;
const C: usize = 1;
const R: usize = 2;

fn gates(z: f32, a: GateOp, b: GateOp, c: GateOp) -> Event {
    Event { z, kind: EventKind::Gates { ops: [a, b, c] } }
}

fn pcrate(z: f32, lane: usize, hp: f32, reward: PowerUp) -> Event {
    Event { z, kind: EventKind::Crate { lane, hp, reward } }
}

/// 指定レーンそれぞれに `count` 体ずつ配置
fn swarm(ev: &mut Vec<Event>, z: f32, kind: EnemyKind, lanes: &[usize], count: u32, depth: f32) {
    for &l in lanes {
        ev.push(Event {
            z,
            kind: EventKind::Swarm { kind, x: lane_x(l), width: LANE_W - 0.8, depth, count, hp_mul: 1.0 },
        });
    }
}

pub fn build() -> Stage {
    use EnemyKind::*;
    use GateOp::*;
    use PowerUp::*;

    let mut ev = vec![
        // 序盤: 増やして最初のパワーアップを取る
        gates(14.0, Add(3), Add(-3), Add(5)),
        gates(30.0, Mul(2), Add(6), Add(-5)),
        pcrate(42.0, L, 30.0, Damage),
        pcrate(42.0, R, 30.0, FireRate),
        gates(80.0, Add(-5), Add(8), Add(4)),
        pcrate(94.0, C, 80.0, Pierce),
        gates(130.0, Add(10), Add(6), Add(-10)),
        pcrate(144.0, R, 130.0, Damage),
        gates(192.0, Add(-15), Add(10), Add(12)),
        pcrate(206.0, L, 200.0, FireRate),
        pcrate(206.0, R, 200.0, Spread),
        gates(246.0, Add(15), Mul(2), Add(-20)),
        Event { z: 268.0, kind: EventKind::Boss { hp_mul: 7.5 } },
    ];
    swarm(&mut ev, 60.0, Grunt, &[C], 12, 5.0);
    swarm(&mut ev, 110.0, Grunt, &[L, R], 14, 6.0);
    swarm(&mut ev, 168.0, Brute, &[C], 1, 0.0);
    swarm(&mut ev, 168.0, Grunt, &[L, R], 12, 6.0);
    swarm(&mut ev, 224.0, Grunt, &[L, C, R], 22, 8.0);
    ev.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap());

    Stage {
        name: "Stage 1",
        theme: Theme { sea: [0.08, 0.34, 0.5], road: [0.58, 0.6, 0.65] },
        grunt_tex: [0, 1],
        boss_tex: 0,
        boss_tempo: 1.0,
        boss_patterns: vec![BossPattern::Safe],
        start_units: 10, finish_z: 262.0, events: ev }
}
