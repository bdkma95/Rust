use rand::{rng, Rng};
use rand::seq::IndexedRandom;

pub struct Cipher {
    key: String,
}

impl Cipher {
    fn random_key() -> String {
        let mut rng = rng();
        let charset: Vec<char> = ('a'..='z').collect();
        (0..100)
            .map(|_| *charset.choose(&mut rng).unwrap())
            .collect()
    }

    fn is_valid_key(key: &str) -> bool {
        !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase())
    }

    pub fn new(key: Option<&str>) -> Self {
        match key {
            Some(k) if Self::is_valid_key(k) => Self { key: k.to_string() },
            _ => Self { key: Self::random_key() },
        }
    }

    pub fn encode(&self, plaintext: &str) -> String {
        plaintext
            .chars()
            .zip(self.key.chars().cycle())
            .map(|(pt, k)| {
                let shift = k as u8 - b'a';
                (((pt as u8 - b'a' + shift) % 26) + b'a') as char
            })
            .collect()
    }

    pub fn decode(&self, ciphertext: &str) -> String {
        ciphertext
            .chars()
            .zip(self.key.chars().cycle())
            .map(|(ct, k)| {
                let shift = k as u8 - b'a';
                (((ct as u8 - b'a' + 26 - shift) % 26) + b'a') as char
            })
            .collect()
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

// ✅ Add free functions to match test expectations
pub fn encode(key: &str, plaintext: &str) -> Option<String> {
    if Cipher::is_valid_key(key) && plaintext.chars().all(|c| c.is_ascii_lowercase()) {
        Some(Cipher::new(Some(key)).encode(plaintext))
    } else {
        None
    }
}

pub fn decode(key: &str, ciphertext: &str) -> Option<String> {
    if Cipher::is_valid_key(key) && ciphertext.chars().all(|c| c.is_ascii_lowercase()) {
        Some(Cipher::new(Some(key)).decode(ciphertext))
    } else {
        None
    }
}

pub fn encode_random(plaintext: &str) -> (String, String) {
    let cipher = Cipher::new(None);
    let encoded = cipher.encode(plaintext);
    (cipher.key().to_string(), encoded)
}
