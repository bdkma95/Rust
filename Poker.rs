use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
enum HandRank {
    HighCard(Vec<u8>),
    OnePair(u8, Vec<u8>),
    TwoPair(u8, u8, u8),
    ThreeOfAKind(u8, Vec<u8>),
    Straight(u8),
    Flush(Vec<u8>),
    FullHouse(u8, u8),
    FourOfAKind(u8, u8),
    StraightFlush(u8),
}

pub fn winning_hands<'a>(hands: &[&'a str]) -> Vec<&'a str> {
    let ranked: Vec<(&'a str, HandRank)> = hands.iter()
        .map(|&h| (h, rank_hand(h)))
        .collect();

    let max_rank = ranked.iter().max_by_key(|(_, r)| r).map(|(_, r)| r.clone());

    ranked.into_iter()
        .filter(|(_, r)| Some(r) == max_rank.as_ref())
        .map(|(h, _)| h)
        .collect()
}

fn rank_hand(hand: &str) -> HandRank {
    let mut values = Vec::new();
    let mut suits = Vec::new();

    for card in hand.split_whitespace() {
        let (v, s) = card.split_at(card.len() - 1);
        values.push(parse_value(v));
        suits.push(s.chars().next().unwrap());
    }

    values.sort_unstable_by(|a, b| b.cmp(a)); // descending
    let is_flush = suits.iter().all(|&s| s == suits[0]);
    let mut is_straight = values.windows(2).all(|w| w[0] == w[1] + 1);

    // Ace-low straight
    if values == vec![14, 5, 4, 3, 2] {
        is_straight = true;
        values = vec![5, 4, 3, 2, 1];
    }

    let mut counts = HashMap::new();
    for &v in &values {
        *counts.entry(v).or_insert(0) += 1;
    }

    let mut count_vec: Vec<_> = counts.iter().collect();
    count_vec.sort_by(|a, b| b.1.cmp(a.1).then_with(|| b.0.cmp(a.0)));

    match (is_flush, is_straight, count_vec.as_slice()) {
        (true, true, _) => HandRank::StraightFlush(values[0]),
        (_, _, &[(v, &4), (k, &1)]) => HandRank::FourOfAKind(*v, *k),
        (_, _, &[(v3, &3), (v2, &2)]) => HandRank::FullHouse(*v3, *v2),
        (true, false, _) => HandRank::Flush(values.clone()),
        (false, true, _) => HandRank::Straight(values[0]),
        (_, _, &[(v, &3), (_, &1), (_, &1)]) =>
            HandRank::ThreeOfAKind(*v, kickers(&values, &[*v])),
        (_, _, &[(p1, &2), (p2, &2), (k, &1)]) =>
            HandRank::TwoPair(*p1, *p2, *k),
        (_, _, &[(p, &2), (_, &1), (_, &1), (_, &1)]) =>
            HandRank::OnePair(*p, kickers(&values, &[*p])),
        _ => HandRank::HighCard(values.clone()),
    }
}

fn parse_value(v: &str) -> u8 {
    match v {
        "A" => 14,
        "K" => 13,
        "Q" => 12,
        "J" => 11,
        "T" => 10,
        _ => v.parse().unwrap(),
    }
}

fn kickers(values: &[u8], exclude: &[u8]) -> Vec<u8> {
    values.iter().filter(|&&v| !exclude.contains(&v)).cloned().collect()
}
