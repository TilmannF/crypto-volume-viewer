#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    reason = "header candidate tests use direct synthetic reader assertions"
)]

use std::cell::RefCell;
use std::collections::VecDeque;

use cryptovol_core::{BlockReader, CryptovolError};
use cryptovol_tcvc::{
    inspect_header_candidates, CandidateReadStatus, HeaderCandidateRole, HeaderCandidateState,
    TcvcInspection, TCVC_HEADER_CANDIDATE_LEN,
};

struct MemoryReader {
    data: Vec<u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    failures: RefCell<VecDeque<CryptovolError>>,
}

impl MemoryReader {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            reads: RefCell::new(Vec::new()),
            failures: RefCell::new(VecDeque::new()),
        }
    }

    fn with_failure(data: Vec<u8>, failure: CryptovolError) -> Self {
        Self {
            data,
            reads: RefCell::new(Vec::new()),
            failures: RefCell::new(VecDeque::from([failure])),
        }
    }

    fn read_log(&self) -> Vec<(u64, usize)> {
        self.reads.borrow().clone()
    }
}

impl BlockReader for MemoryReader {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), CryptovolError> {
        self.reads.borrow_mut().push((offset, buf.len()));

        if let Some(failure) = self.failures.borrow_mut().pop_front() {
            return Err(failure);
        }

        let start = offset as usize;
        let end = start
            .checked_add(buf.len())
            .ok_or(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.len(),
            })?;

        if end > self.data.len() {
            return Err(CryptovolError::OutOfBounds {
                offset,
                length: buf.len(),
                file_len: self.len(),
            });
        }

        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
}

#[test]
fn reports_primary_and_backup_candidates_for_normal_file() {
    let reader = MemoryReader::new(vec![0x5a; 1024]);

    let inspection = inspect_header_candidates(&reader).expect("inspection should succeed");

    let TcvcInspection::Candidates { primary, backup } = inspection else {
        panic!("expected candidate inspection for normal file: {inspection:?}");
    };

    assert_eq!(primary.role, HeaderCandidateRole::Primary);
    assert_eq!(primary.offset, 0);
    assert_eq!(primary.length, TCVC_HEADER_CANDIDATE_LEN);
    assert_eq!(primary.read_status, CandidateReadStatus::Readable);

    assert_eq!(
        backup,
        HeaderCandidateState::Candidate {
            role: HeaderCandidateRole::Backup,
            offset: 512,
            length: TCVC_HEADER_CANDIDATE_LEN,
            read_status: CandidateReadStatus::Readable,
        }
    );
}

#[test]
fn reports_empty_file_as_too_small() {
    let reader = MemoryReader::new(Vec::new());

    let inspection = inspect_header_candidates(&reader).expect("inspection should succeed");

    assert_eq!(
        inspection,
        TcvcInspection::TooSmall {
            file_size: 0,
            required_minimum: TCVC_HEADER_CANDIDATE_LEN,
        }
    );
    assert!(reader.read_log().is_empty(), "tiny files must not be read");
}

#[test]
fn reports_sub_header_file_as_too_small() {
    let reader = MemoryReader::new(vec![0x11; 128]);

    let inspection = inspect_header_candidates(&reader).expect("inspection should succeed");

    assert_eq!(
        inspection,
        TcvcInspection::TooSmall {
            file_size: 128,
            required_minimum: TCVC_HEADER_CANDIDATE_LEN,
        }
    );
    assert!(reader.read_log().is_empty(), "tiny files must not be read");
}

#[test]
fn reports_exact_header_sized_file_with_backup_overlap() {
    let reader = MemoryReader::new(vec![0x22; 512]);

    let inspection = inspect_header_candidates(&reader).expect("inspection should succeed");

    let TcvcInspection::Candidates { primary, backup } = inspection else {
        panic!("expected candidate inspection for exact header-sized file");
    };

    assert_eq!(primary.role, HeaderCandidateRole::Primary);
    assert_eq!(primary.offset, 0);
    assert_eq!(primary.length, TCVC_HEADER_CANDIDATE_LEN);
    assert_eq!(primary.read_status, CandidateReadStatus::Readable);
    assert_eq!(backup, HeaderCandidateState::OverlapsPrimary);
    assert_eq!(reader.read_log(), vec![(0, 512)]);
}

#[test]
fn reads_primary_candidate_from_synthetic_data() {
    let mut data = vec![0; 1024];
    data[..512].fill(0xa5);
    let reader = MemoryReader::new(data);

    let inspection = inspect_header_candidates(&reader).expect("inspection should succeed");

    let TcvcInspection::Candidates { primary, .. } = inspection else {
        panic!("expected candidate inspection");
    };

    assert_eq!(primary.read_status, CandidateReadStatus::Readable);
    assert!(
        reader.read_log().contains(&(0, 512)),
        "primary candidate should be read at offset 0"
    );
}

#[test]
fn reads_backup_candidate_from_synthetic_data() {
    let mut data = vec![0; 1536];
    data[1024..1536].fill(0x3c);
    let reader = MemoryReader::new(data);

    let inspection = inspect_header_candidates(&reader).expect("inspection should succeed");

    let TcvcInspection::Candidates { backup, .. } = inspection else {
        panic!("expected candidate inspection");
    };

    assert_eq!(
        backup,
        HeaderCandidateState::Candidate {
            role: HeaderCandidateRole::Backup,
            offset: 1024,
            length: TCVC_HEADER_CANDIDATE_LEN,
            read_status: CandidateReadStatus::Readable,
        }
    );
    assert!(
        reader.read_log().contains(&(1024, 512)),
        "backup candidate should be read from the last 512 bytes"
    );
}

#[test]
fn reports_bounds_violating_read_failure_as_structured_metadata() {
    let reader = MemoryReader::with_failure(
        vec![0x44; 1024],
        CryptovolError::OutOfBounds {
            offset: 0,
            length: 512,
            file_len: 128,
        },
    );

    let inspection = inspect_header_candidates(&reader).expect("inspection should succeed");

    let TcvcInspection::Candidates { primary, .. } = inspection else {
        panic!("expected candidate inspection");
    };

    assert_eq!(
        primary.read_status,
        CandidateReadStatus::ReadFailed {
            error: "read range offset 0 length 512 is outside file length 128".to_string(),
        }
    );
}
