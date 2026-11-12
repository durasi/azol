// bignum: a large factorial with a list of limbs.
// Algorithm: multiply a number kept as base-1,000,000,000 limbs by 2, 3,
// ... n, carrying as it goes.
// Sizes: 24,000!; small: 2,000!.
// Output: the number of limbs and the sum of the decimal digits.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

const BASE: i64 = 1000000000;

fn main() {
    let n = size(24000, 2000);
    let mut limbs: Vec<i64> = vec![1];
    for factor in 2..=n {
        let mut carry = 0;
        for limb in limbs.iter_mut() {
            let product = *limb * factor + carry;
            *limb = product % BASE;
            carry = product / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    let mut digits = 0;
    for &limb in &limbs {
        let mut rest = limb;
        while rest > 0 {
            digits += rest % 10;
            rest /= 10;
        }
    }
    println!("{} {}", limbs.len(), digits);
}
