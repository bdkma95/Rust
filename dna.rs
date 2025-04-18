use std::collections::HashMap;

const VALID_NUCLEOTIDES: [char; 4] = ['A', 'C', 'G', 'T'];

pub fn count(nucleotide: char, dna: &str) -> Result<usize, char> {
    if !VALID_NUCLEOTIDES.contains(&nucleotide) {
        return Err(nucleotide);
    }

    for c in dna.chars() {
        if !VALID_NUCLEOTIDES.contains(&c) {
            return Err(c);
        }
    }

    Ok(dna.chars().filter(|&c| c == nucleotide).count())
}

pub fn nucleotide_counts(dna: &str) -> Result<HashMap<char, usize>, char> {
    let mut counts = HashMap::from([
        ('A', 0),
        ('C', 0),
        ('G', 0),
        ('T', 0),
    ]);

    for c in dna.chars() {
        if !VALID_NUCLEOTIDES.contains(&c) {
            return Err(c);
        }
        *counts.get_mut(&c).unwrap() += 1;
    }

    Ok(counts)
}
