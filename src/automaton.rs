//! Fuzzy street matching. An FST automaton computing Damerau-Levenshtein distance
//! one row at a time, so memory is a few short rows, not a prebuilt DFA, and a
//! swapped pair of letters costs one edit instead of two.

use fst::Automaton;

pub(crate) struct Fuzzy<'a> {
    query: &'a [u8],
    max: u8,
}

impl<'a> Fuzzy<'a> {
    pub(crate) fn new(query: &'a str, max: u8) -> Self {
        Self {
            query: query.as_bytes(),
            max,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Row {
    previous: Vec<u8>,
    current: Vec<u8>,
    last: u8,
}

impl Automaton for Fuzzy<'_> {
    type State = Option<Row>;

    fn start(&self) -> Self::State {
        let current = (0..=self.query.len()).map(|i| i.min(255) as u8).collect();
        Some(Row {
            previous: Vec::new(),
            current,
            last: 0,
        })
    }

    fn is_match(&self, state: &Self::State) -> bool {
        state
            .as_ref()
            .is_some_and(|r| r.current[self.query.len()] <= self.max)
    }

    fn can_match(&self, state: &Self::State) -> bool {
        state.is_some()
    }

    fn accept(&self, state: &Self::State, byte: u8) -> Self::State {
        let row = state.as_ref()?;
        let n = self.query.len();
        let mut next = Vec::with_capacity(n + 1);
        next.push(row.current[0].saturating_add(1));
        for j in 1..=n {
            let substitute = row.current[j - 1] + u8::from(self.query[j - 1] != byte);
            let mut best = substitute
                .min(row.current[j].saturating_add(1))
                .min(next[j - 1].saturating_add(1));
            let swapped = j > 1
                && !row.previous.is_empty()
                && self.query[j - 1] == row.last
                && self.query[j - 2] == byte;
            if swapped {
                best = best.min(row.previous[j - 2].saturating_add(1));
            }
            next.push(best);
        }
        // A swap reads the previous row, so a branch stays alive one row longer.
        let alive =
            next.iter().any(|&d| d <= self.max) || row.current.iter().any(|&d| d < self.max);
        alive.then(|| Row {
            previous: row.current.clone(),
            current: next,
            last: byte,
        })
    }
}

/// Edit distance, with a swapped pair of letters counting once, for ranking matches.
pub(crate) fn edits(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    rows[0] = (0..=b.len()).collect();
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}
