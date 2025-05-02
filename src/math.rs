#[macro_export]
macro_rules! check_radial_collision {
    ($a:expr, $b:expr) => {{
        let dx = $a.x - $b.x;
        let dy = $a.y - $b.y;
        let dist_sq = dx * dx + dy * dy;
        let rsum = $a.radius + $b.radius;
        dist_sq <= rsum * rsum
    }};
}
#[macro_export]
macro_rules! center_within_larger {
    ($a:expr, $b:expr) => {{
        let dx = $a.x - $b.x;
        let dy = $a.y - $b.y;
        let dist_sq = dx * dx + dy * dy;
        let max_r = $a.radius.max($b.radius);
        dist_sq < max_r * max_r
    }};
}

pub fn clamp(num: f64, amount: f64) -> f64 {
    if num > amount {
        return num - amount;
    } else if num < -amount {
        return num + amount;
    } else {
        return 0.0;
    };
}
