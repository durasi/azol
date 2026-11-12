// Run-time benchmark: the same computation as bench/sort.azl.

function quicksort(xs, low, high) {
    if (low >= high) {
        return;
    }
    const pivot = xs[Math.trunc((low + high) / 2)];
    let i = low;
    let j = high;
    while (i <= j) {
        while (xs[i] < pivot) {
            i += 1;
        }
        while (xs[j] > pivot) {
            j -= 1;
        }
        if (i <= j) {
            const held = xs[i];
            xs[i] = xs[j];
            xs[j] = held;
            i += 1;
            j -= 1;
        }
    }
    quicksort(xs, low, j);
    quicksort(xs, i, high);
}

const count = 2000000;
const xs = [];
let seed = 42;
let n = 0;
while (n < count) {
    seed = seed * 48271 % 2147483647;
    xs.push(seed % 1000000);
    n += 1;
}
quicksort(xs, 0, count - 1);
let checksum = 0;
let k = 0;
while (k < count) {
    checksum += xs[k] * (k % 7);
    k += 1000;
}
let ordered = 1;
let m = 1;
while (m < count) {
    if (xs[m - 1] > xs[m]) {
        ordered = 0;
    }
    m += 1;
}
console.log(checksum + ordered);
