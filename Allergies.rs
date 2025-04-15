#[derive(Debug, PartialEq, Eq)]
pub enum Allergen {
    Eggs,
    Peanuts,
    Shellfish,
    Strawberries,
    Tomatoes,
    Chocolate,
    Pollen,
    Cats,
}

pub struct Allergies {
    score: u32,
}

impl Allergies {
    // Constructor that accepts a score and returns a new Allergies instance
    pub fn new(score: u32) -> Self {
        Allergies { score }
    }

    // Method to determine if the patient is allergic to a specific allergen
    pub fn is_allergic_to(&self, allergen: &Allergen) -> bool {
        let allergen_bit = match allergen {
            Allergen::Eggs => 0,
            Allergen::Peanuts => 1,
            Allergen::Shellfish => 2,
            Allergen::Strawberries => 3,
            Allergen::Tomatoes => 4,
            Allergen::Chocolate => 5,
            Allergen::Pollen => 6,
            Allergen::Cats => 7,
        };
        
        // Check if the bit corresponding to the allergen is set in the score
        (self.score & (1 << allergen_bit)) != 0
    }

    // Method to return a list of allergens the patient is allergic to
    pub fn allergies(&self) -> Vec<Allergen> {
        let mut allergens = Vec::new();
        
        if self.is_allergic_to(&Allergen::Eggs) {
            allergens.push(Allergen::Eggs);
        }
        if self.is_allergic_to(&Allergen::Peanuts) {
            allergens.push(Allergen::Peanuts);
        }
        if self.is_allergic_to(&Allergen::Shellfish) {
            allergens.push(Allergen::Shellfish);
        }
        if self.is_allergic_to(&Allergen::Strawberries) {
            allergens.push(Allergen::Strawberries);
        }
        if self.is_allergic_to(&Allergen::Tomatoes) {
            allergens.push(Allergen::Tomatoes);
        }
        if self.is_allergic_to(&Allergen::Chocolate) {
            allergens.push(Allergen::Chocolate);
        }
        if self.is_allergic_to(&Allergen::Pollen) {
            allergens.push(Allergen::Pollen);
        }
        if self.is_allergic_to(&Allergen::Cats) {
            allergens.push(Allergen::Cats);
        }
        
        allergens
    }
}
