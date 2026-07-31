#![no_main]

use libfuzzer_sys::fuzz_target;
use shipment::fuzz_api;

#[derive(Debug)]
struct Input {
    a: i128,
    b: i128,
    c: i128,
}

fn parse(data: &[u8]) -> Option<Input> {
    if data.len() < 48 {
        return None;
    }
    let a = i128::from_le_bytes(data[0..16].try_into().unwrap());
    let b = i128::from_le_bytes(data[16..32].try_into().unwrap());
    let c = i128::from_le_bytes(data[32..48].try_into().unwrap());
    Some(Input { a, b, c })
}

fuzz_target!(|data: &[u8]| {
    let Some(Input { a, b, c }) = parse(data) else {
        return;
    };

    // Addition must either match checked native addition or report an
    // arithmetic error — it must never panic or silently wrap.
    match fuzz_api::add_i128(a, b) {
        Ok(sum) => assert_eq!(Some(sum), a.checked_add(b)),
        Err(_) => assert!(a.checked_add(b).is_none()),
    }

    // Subtraction must either match checked native subtraction or report an
    // arithmetic error.
    match fuzz_api::sub_i128(a, b) {
        Ok(diff) => assert_eq!(Some(diff), a.checked_sub(b)),
        Err(_) => assert!(a.checked_sub(b).is_none()),
    }

    // Escrow subtraction must never produce a negative escrow balance.
    match fuzz_api::sub_escrow(a, b) {
        Ok(diff) => assert!(diff >= 0),
        Err(_) => { /* overflow or negative result correctly rejected */ }
    }

    // Multiply-then-divide must never panic on overflow or division by zero.
    let _ = fuzz_api::mul_div_i128(a, b, c);
});
