//! Reading every row of one store in a scope, and what to do with the ones that will not decode.
//!
//! Split out of [`super::store`] when it passed the four-hundred-line cap `AGENTS.md` sets. The
//! line is a real seam rather than a length: everything next door implements a store trait, and
//! this is the one primitive all of them are built on.

use frontbox::{Error, ScopeKey};
use wasm_bindgen::JsValue;
use web_sys::IdbObjectStore;

use crate::convert::from_js;
use crate::request::{await_request, js_error};

/// A stored entry that did not match the row type asked for, kept with what is needed to move it.
///
/// `key` is the object store's **primary** key, not the index key: `IDBIndex.getAllKeys` returns
/// primary keys, which is exactly what a delete needs. Reading it from the object itself is not an
/// option here — the object is the thing that would not parse.
pub(crate) struct RawEntry {
    pub key: JsValue,
    /// The stored object as JSON text, so the evidence survives quarantine.
    pub text: String,
}

/// A stored entry that decoded, kept beside the key it is stored under.
///
/// The key travels with the row for the same reason it travels with a [`RawEntry`]: every mutating
/// path here — a sweep, a purge, an outcome — has to *delete* what it read, and an object store
/// value does not carry its own primary key. Reconstructing one from a field is a guess, and a
/// guess that lands on the wrong row deletes somebody's queued write.
pub(crate) struct Decoded<T> {
    pub key: JsValue,
    pub row: T,
}

/// Every row of one store in this scope, split into what decoded and what did not.
pub(crate) struct ScopeScan<T> {
    pub rows: Vec<Decoded<T>>,
    pub undecodable: Vec<RawEntry>,
}

impl<T> ScopeScan<T> {
    /// The rows that decoded, accepting that anything which did not is invisible here.
    ///
    /// **Only correct where `sweep_corrupt` can still reach what this drops.** That is the outbox,
    /// and it is the contract `OutboxStore::pending_batch` states: undecodable rows are not
    /// returned by reads and become visible by being swept into quarantine. A caller that uses this
    /// on a store with no sweep is choosing to lose the row, so each such call site says why.
    ///
    /// Drops the keys with it, which is the honest shape for a read: a caller that only renders
    /// rows has nothing to delete, and one that does should be holding [`Decoded`] instead.
    pub(crate) fn decodable(self) -> Vec<T> {
        self.rows.into_iter().map(|decoded| decoded.row).collect()
    }
}

/// Read every row of one store belonging to this scope.
///
/// `get_all` on the scope index rather than a cursor walk: one request instead of one per row, and
/// **one request is what keeps the transaction alive**. A cursor loop awaits between steps, which
/// is exactly where a transaction goes inactive if anything else is scheduled in between.
///
/// # Why the failures come back instead of being filtered out
///
/// This used to be `filter_map(from_js)`, and a row that did not match `T` simply vanished. For the
/// outbox that was not merely a lossy read, it was an **unreachable row**: absent from
/// `pending_batch` (correct — that is the contract), absent from `pending_count` (correct), and
/// absent from `sweep_corrupt`'s scan (**not** correct), which is the one path that exists to make
/// such a row visible. It stayed in storage forever, counted by nothing and named by nothing.
///
/// The SQLite adapter cannot reach that state: its rows are read column by column into a raw
/// struct, so a row is always *readable* and only ever fails the later semantic `decode`. This is
/// the same property, reconstructed for a store that holds whole objects — a stored object that
/// does not match its row type is a decode failure like any other, and it now has somewhere to go.
///
/// The keys are fetched alongside the values because a row that will not parse cannot be asked for
/// its own primary key. `getAll` and `getAllKeys` over the same index and the same query return
/// corresponding orders, which is what lets the two arrays be zipped.
pub(crate) async fn all_in_scope<T>(
    store: &IdbObjectStore,
    scope: &ScopeKey,
) -> Result<ScopeScan<T>, Error>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let index = store.index("scope").map_err(js_error)?;
    let key = JsValue::from_str(scope.as_str());
    let values = await_request(index.get_all_with_key(&key).map_err(js_error)?).await?;
    let keys = await_request(index.get_all_keys_with_key(&key).map_err(js_error)?).await?;
    let values = js_sys::Array::from(&values);
    let keys = js_sys::Array::from(&keys);

    // The two arrays are index-aligned by specification — same index, same query, corresponding
    // orders — and everything below reads a key by position. If that ever stopped holding, the
    // failure would be silent and destructive rather than loud: `Array::get` past the end returns
    // `undefined`, which `delete` accepts as a key range that matches nothing, so a sweep would
    // report rows it had not actually removed.
    debug_assert_eq!(
        values.length(),
        keys.length(),
        "getAll and getAllKeys over one index and one query must correspond"
    );

    let mut rows = Vec::with_capacity(values.length() as usize);
    let mut undecodable = Vec::new();
    for (position, item) in values.iter().enumerate() {
        let key = keys.get(position as u32);
        match from_js::<T>(&item) {
            Some(row) => rows.push(Decoded { key, row }),
            None => undecodable.push(RawEntry {
                key,
                text: js_sys::JSON::stringify(&item)
                    .ok()
                    .and_then(|text| text.as_string())
                    // A value that will not even stringify is one `JSON.stringify` refuses —
                    // a cycle, or a `BigInt`. The row still has to be nameable, so it gets a
                    // description instead of being dropped a second time.
                    .unwrap_or_else(|| "<unstringifiable stored value>".to_owned()),
            }),
        }
    }
    Ok(ScopeScan { rows, undecodable })
}
