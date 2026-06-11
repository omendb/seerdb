//! Transaction management for MVCC.
//!
//! Transactions provide snapshot isolation. Each transaction sees a consistent
//! view of the database as of its start time. Writers don't block readers.

use std::sync::atomic::{AtomicU64, Ordering};

/// Transaction ID type.
pub type TransactionId = u64;

/// Transaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// Transaction is active.
    Active,
    /// Transaction has been committed.
    Committed,
    /// Transaction has been aborted.
    Aborted,
}

/// A transaction providing snapshot isolation.
///
/// Each transaction has:
/// - A unique ID
/// - A snapshot ID (the state of the database it can see)
/// - A state (active, committed, aborted)
pub struct Transaction {
    /// Unique transaction ID.
    id: TransactionId,
    /// Snapshot ID: the latest committed transaction ID at start time.
    snapshot_id: TransactionId,
    /// Current state.
    state: TransactionState,
    /// Read set: pages read during this transaction (for optimistic validation).
    read_set: Vec<u64>,
    /// Write set: pages written during this transaction.
    write_set: Vec<u64>,
}

impl Transaction {
    /// Create a new transaction.
    pub fn new(id: TransactionId, snapshot_id: TransactionId) -> Self {
        Self {
            id,
            snapshot_id,
            state: TransactionState::Active,
            read_set: Vec::new(),
            write_set: Vec::new(),
        }
    }

    /// Get the transaction ID.
    pub fn id(&self) -> TransactionId {
        self.id
    }

    /// Get the snapshot ID.
    pub fn snapshot_id(&self) -> TransactionId {
        self.snapshot_id
    }

    /// Get the transaction state.
    pub fn state(&self) -> TransactionState {
        self.state
    }

    /// Whether the transaction is active.
    pub fn is_active(&self) -> bool {
        self.state == TransactionState::Active
    }

    /// Record a page read (for optimistic validation).
    pub fn record_read(&mut self, page_id: u64) {
        if self.state == TransactionState::Active {
            self.read_set.push(page_id);
        }
    }

    /// Record a page write.
    pub fn record_write(&mut self, page_id: u64) {
        if self.state == TransactionState::Active {
            self.write_set.push(page_id);
        }
    }

    /// Get the read set.
    pub fn read_set(&self) -> &[u64] {
        &self.read_set
    }

    /// Get the write set.
    pub fn write_set(&self) -> &[u64] {
        &self.write_set
    }

    /// Commit the transaction.
    pub fn commit(&mut self) {
        self.state = TransactionState::Committed;
    }

    /// Abort the transaction.
    pub fn abort(&mut self) {
        self.state = TransactionState::Aborted;
    }
}

/// Transaction manager: allocates transaction IDs and tracks state.
#[allow(dead_code)]
pub struct TransactionManager {
    /// Next transaction ID.
    next_id: AtomicU64,
    /// ID of the latest committed transaction.
    latest_committed: AtomicU64,
}

#[allow(dead_code)]
impl TransactionManager {
    /// Create a new transaction manager.
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            latest_committed: AtomicU64::new(0),
        }
    }

    /// Begin a new transaction.
    pub fn begin(&self) -> Transaction {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let snapshot_id = self.latest_committed.load(Ordering::Acquire);
        Transaction::new(id, snapshot_id)
    }

    /// Commit a transaction.
    pub fn commit(&self, txn: &mut Transaction) {
        txn.commit();
        self.latest_committed.store(txn.id(), Ordering::Release);
    }

    /// Abort a transaction.
    pub fn abort(&self, txn: &mut Transaction) {
        txn.abort();
    }

    /// Get the latest committed transaction ID.
    pub fn latest_committed(&self) -> TransactionId {
        self.latest_committed.load(Ordering::Acquire)
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_lifecycle() {
        let tm = TransactionManager::new();
        let mut txn = tm.begin();

        assert!(txn.is_active());
        assert_eq!(txn.state(), TransactionState::Active);

        txn.record_read(1);
        txn.record_write(2);
        assert_eq!(txn.read_set(), &[1]);
        assert_eq!(txn.write_set(), &[2]);

        tm.commit(&mut txn);
        assert_eq!(txn.state(), TransactionState::Committed);
        assert!(!txn.is_active());
    }

    #[test]
    fn test_transaction_abort() {
        let tm = TransactionManager::new();
        let mut txn = tm.begin();

        tm.abort(&mut txn);
        assert_eq!(txn.state(), TransactionState::Aborted);
    }

    #[test]
    fn test_snapshot_isolation() {
        let tm = TransactionManager::new();

        // Begin first transaction.
        let txn1 = tm.begin();
        assert_eq!(txn1.snapshot_id(), 0); // no committed txns yet

        // Commit first transaction.
        let mut txn1 = txn1;
        tm.commit(&mut txn1);

        // Begin second transaction - should see txn1's commit.
        let txn2 = tm.begin();
        assert_eq!(txn2.snapshot_id(), txn1.id());
    }

    #[test]
    fn test_concurrent_transactions() {
        let tm = TransactionManager::new();

        let txn1 = tm.begin();
        let txn2 = tm.begin();

        assert_ne!(txn1.id(), txn2.id());
        assert_eq!(txn1.snapshot_id(), txn2.snapshot_id());
    }

    #[test]
    fn test_transaction_id_ordering() {
        let tm = TransactionManager::new();

        let txn1 = tm.begin();
        let txn2 = tm.begin();
        let txn3 = tm.begin();

        assert!(txn1.id() < txn2.id());
        assert!(txn2.id() < txn3.id());
    }
}
