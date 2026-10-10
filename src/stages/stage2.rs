use super::*;

const L: usize = 0;
const C: usize = 1;
const R: usize = 2;

pub fn build() -> Stage {
    use EnemyKind::*;
    use GateOp::*;
    use PowerUp::*;

    let mut ev = vec![
        gates(12.0, Add(4), Add(-4), Add(6)),
        gates(28.0, Add(-6), Add(7), Add(5)),
        pcrate(40.0, L, 50.0, Damage),
        pcrate(40.0, R, 50.0, FireRate),
        gates(78.0, Add(10), Add(-8), Add(8)),
        pcrate(92.0, C, 120.0, Pierce),
        gates(128.0, Add(12), Add(-10), Add(6)),
        pcrate(140.0, L, 200.0, Damage),
        pcrate(140.0, R, 200.0, Spread),
        gates(182.0, Add(-15), Add(15), Add(12)),
        pcrate(196.0, C, 260.0, FireRate),
        gates(238.0, Add(14), Add(-20), Add(18)),
        pcrate(252.0, L, 320.0, Damage),
        pcrate(252.0, R, 320.0, Pierce),
        gates(296.0, Add(30), Mul(2), Add(-25)),
        Event { z: 318.0, kind: EventKind::Boss { hp_mul: 11.5 } },
    ];
    swarm_hp(&mut ev, 56.0, Grunt, &[C, R], 14, 6.0, 1.0);
    swarm_hp(&mut ev, 108.0, Grunt, &[L, C, R], 12, 6.0, 1.3);
    swarm_hp(&mut ev, 160.0, Brute, &[C], 1, 0.0, 1.0);
    swarm_hp(&mut ev, 160.0, Grunt, &[L, R], 14, 6.0, 1.3);
    swarm_hp(&mut ev, 214.0, Brute, &[L, R], 1, 0.0, 1.0);
    swarm_hp(&mut ev, 214.0, Grunt, &[C], 30, 8.0, 1.5);
    swarm_hp(&mut ev, 272.0, Grunt, &[L, C, R], 30, 9.0, 1.6);
    ev.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap());

    Stage {
        name: "Stage 2",
        theme: Theme { sea: [0.22, 0.1, 0.38], road: [0.46, 0.42, 0.55] },
        grunt_tex: [2, 0],
        boss_tex: 1,
        boss_tempo: 0.6,
        boss_patterns: vec![BossPattern::Safe, BossPattern::SweepLR, BossPattern::SweepRL, BossPattern::Pincer, BossPattern::Shift],
        start_units: 10,
        finish_z: 312.0,
        events: ev,
    }
}
