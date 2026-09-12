//! Feeds the parser thousands of random byte-level corruptions of a valid
//! AFM file. There's no crate for this (no dependencies allowed), so this
//! is a small deterministic xorshift PRNG rather than a real fuzzer, but it
//! covers the same failure mode a real fuzzer would first find: a corrupted
//! byte sequence that makes the parser panic (index out of bounds, bad UTF-8
//! slice boundary, etc.) instead of returning a `ParseError`. Panicking on
//! untrusted input is the bug this test exists to catch; a wrong or
//! surprising `ParseError` is not, so the only assertions are "never panics"
//! and "any error it does return points at a real location".

const VALID_AFM: &str = "StartFontMetrics 4.1\n\
FontName Helvetica\n\
FullName Helvetica\n\
FamilyName Helvetica\n\
Weight Medium\n\
ItalicAngle 0\n\
IsFixedPitch false\n\
FontBBox -166 -225 1000 931\n\
UnderlinePosition -100\n\
UnderlineThickness 50\n\
CapHeight 718\n\
XHeight 523\n\
Ascender 718\n\
Descender -207\n\
StdHW 80\n\
StdVW 88\n\
StartCharMetrics 3\n\
C 32 ; WX 278 ; N space ;\n\
C 65 ; WX 667 ; N A ;\n\
C 102 ; WX 333 ; N f ; L i fi ; L l fl ;\n\
EndCharMetrics\n\
StartKernPairs 2\n\
KPX A V -70\n\
KPX A W -50\n\
EndKernPairs\n\
EndFontMetrics\n";

/// Minimal xorshift64* PRNG. Deterministic across runs so a failure is
/// reproducible from the seed alone instead of needing the corrupted input
/// to be captured and attached to the bug report.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_range(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() % bound as u64) as usize
    }
}

/// Applies a handful of random single-byte edits (replace, delete, insert)
/// to `base`. The result is not guaranteed to be valid UTF-8, which is
/// itself part of what's being tested: `parse` takes a `&str`, so any
/// invalid sequence is repaired with the lossy replacement character before
/// being handed in, the same way a caller reading an untrusted file would
/// need to decide what to do with bytes that aren't valid UTF-8.
fn corrupt(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut bytes = base.to_vec();
    let edits = 1 + rng.next_range(8);
    for _ in 0..edits {
        if bytes.is_empty() {
            break;
        }
        match rng.next_range(3) {
            0 => {
                let i = rng.next_range(bytes.len());
                bytes[i] = rng.next_range(256) as u8;
            }
            1 => {
                let i = rng.next_range(bytes.len());
                bytes.remove(i);
            }
            _ => {
                let i = rng.next_range(bytes.len() + 1);
                bytes.insert(i, rng.next_range(256) as u8);
            }
        }
    }
    bytes
}

#[test]
fn baseline_sample_parses_cleanly() {
    afm_metrics::parse(VALID_AFM).expect("hand written sample should be well formed");
}

#[test]
fn random_byte_corruption_never_panics() {
    let mut rng = Rng(0x9e3779b97f4a7c15);

    for _ in 0..5000 {
        let corrupted = corrupt(&mut rng, VALID_AFM.as_bytes());
        let text = String::from_utf8_lossy(&corrupted).into_owned();

        // The call itself is the test: a panic here fails the test run.
        if let Err(err) = afm_metrics::parse(&text) {
            assert!(err.line >= 1, "error line should be 1-based, got {}", err.line);
            assert!(err.column >= 1, "error column should be 1-based, got {}", err.column);
            assert!(!err.message.is_empty(), "error message should not be empty");
        }
    }
}
