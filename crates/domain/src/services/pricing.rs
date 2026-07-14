//! Market price computation.
//!
//! MVP pricing is the *share of volume*: an outcome's price equals its share of
//! the market's total staked volume. Before any volume exists, prices are split
//! evenly across outcomes. A more sophisticated market maker (LMSR/AMM) can
//! replace this later without touching callers.

use crate::entities::Outcome;
use crate::value_objects::market::Price;

/// Recomputes every outcome's `current_price` in place from the outcomes' own
/// volumes, so the prices always sum to exactly 1.0000 (10 000 ten-thousandths).
///
/// With no volume yet, the whole is split as evenly as possible; the leftover
/// ten-thousandths from integer division are handed to the earliest outcomes so
/// the total lands exactly on 10 000.
pub fn recalculate_prices(outcomes: &mut [Outcome]) {
    let n = outcomes.len();
    if n == 0 {
        return;
    }

    let total_volume: i64 = outcomes.iter().map(Outcome::volume).sum();
    let scale = i64::from(Price::SCALE);

    // Provisional prices from each outcome's share; the remainder is distributed
    // afterwards so the parts always add up to the whole.
    let mut shares: Vec<i64> = if total_volume <= 0 {
        let base = scale / n as i64;
        vec![base; n]
    } else {
        outcomes
            .iter()
            .map(|o| o.volume() * scale / total_volume)
            .collect()
    };

    let assigned: i64 = shares.iter().sum();
    let mut remainder = scale - assigned;
    // `remainder` is in `0..n` for both branches; hand one unit to each of the
    // first `remainder` outcomes.
    let mut i = 0;
    while remainder > 0 {
        shares[i] += 1;
        remainder -= 1;
        i += 1;
    }

    for (outcome, share) in outcomes.iter_mut().zip(shares) {
        // `share` is in `0..=SCALE` by construction, so this never fails.
        if let Ok(price) = Price::from_ten_thousandths(share as i32) {
            outcome.set_current_price(price);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{MarketId, Outcome};
    use crate::value_objects::market::{OutcomeLabel, Price};

    fn outcome(market: MarketId, volume: i64) -> Outcome {
        Outcome::from_parts(
            crate::entities::OutcomeId::new(),
            market,
            OutcomeLabel::new("opt").unwrap(),
            Price::ZERO,
            volume,
        )
    }

    fn sum(outcomes: &[Outcome]) -> i32 {
        outcomes
            .iter()
            .map(|o| o.current_price().as_ten_thousandths())
            .sum()
    }

    #[test]
    fn even_split_when_no_volume() {
        let market = MarketId::new();
        let mut outcomes = vec![outcome(market, 0), outcome(market, 0), outcome(market, 0)];
        recalculate_prices(&mut outcomes);
        // 10000 / 3 = 3334, 3333, 3333 -> sums to 10000.
        assert_eq!(sum(&outcomes), Price::SCALE);
        let prices: Vec<i32> = outcomes
            .iter()
            .map(|o| o.current_price().as_ten_thousandths())
            .collect();
        assert_eq!(prices, vec![3334, 3333, 3333]);
    }

    #[test]
    fn prices_track_volume_share_and_sum_to_one() {
        let market = MarketId::new();
        let mut outcomes = vec![outcome(market, 3_000), outcome(market, 1_000)];
        recalculate_prices(&mut outcomes);
        assert_eq!(sum(&outcomes), Price::SCALE);
        assert_eq!(outcomes[0].current_price().as_ten_thousandths(), 7_500);
        assert_eq!(outcomes[1].current_price().as_ten_thousandths(), 2_500);
    }

    #[test]
    fn remainder_keeps_total_exact() {
        let market = MarketId::new();
        // 100 / 300, 100 / 300, 100 / 300 -> 3333 each, remainder 1.
        let mut outcomes = vec![
            outcome(market, 100),
            outcome(market, 100),
            outcome(market, 100),
        ];
        recalculate_prices(&mut outcomes);
        assert_eq!(sum(&outcomes), Price::SCALE);
    }
}
