// fannkuch: permutations and reversals of a small list.
// Algorithm: for every permutation of 0..n, count how many prefix
// reversals bring the first element to 0 (the classic "fannkuch-redux").
// Sizes: n = 10; small: n = 9.
// Output: the alternating checksum and the largest number of reversals.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let n = size(10, 9) as usize;
    let mut first: Vec<usize> = (0..n).collect();
    let mut perm = vec![0usize; n];
    let mut count = vec![0usize; n];
    let mut most = 0;
    let mut checksum: i64 = 0;
    let mut odd = false;
    let mut r = n;
    loop {
        while r != 1 {
            count[r - 1] = r;
            r -= 1;
        }
        perm.copy_from_slice(&first);
        let mut flips = 0;
        let mut k = perm[0];
        while k != 0 {
            perm[..=k].reverse();
            flips += 1;
            k = perm[0];
        }
        most = most.max(flips);
        checksum += if odd { -flips } else { flips };
        odd = !odd;
        loop {
            if r == n {
                println!("{} {}", checksum, most);
                return;
            }
            first[..=r].rotate_left(1);
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
    }
}
