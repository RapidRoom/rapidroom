use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(crate) type CancelToken = (Arc<AtomicUsize>, usize);
static GENERATION: LazyLock<Arc<AtomicUsize>> = LazyLock::new(|| Arc::new(AtomicUsize::new(0)));

#[tauri::command]
pub fn begin_culling_preview_batch() -> usize {
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

pub(crate) fn token(generation: usize) -> CancelToken {
    (Arc::clone(&GENERATION), generation)
}

pub(crate) fn ensure_current(token: &Option<CancelToken>) -> Result<(), String> {
    if token
        .as_ref()
        .is_some_and(|(counter, generation)| counter.load(Ordering::SeqCst) != *generation)
    {
        return Err("Culling preview cancelled".into());
    }
    Ok(())
}

pub(crate) async fn acquire_decode_permit(
    semaphore: Arc<Semaphore>,
    token: &Option<CancelToken>,
) -> Result<OwnedSemaphorePermit, String> {
    ensure_current(token)?;
    let permit = semaphore.acquire_owned().await.map_err(|e| e.to_string())?;
    ensure_current(token)?;
    Ok(permit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn superseded_waiter_releases_decode_slot_without_starting_work() {
        let semaphore = Arc::new(Semaphore::new(1));
        let held = semaphore.clone().acquire_owned().await.unwrap();
        let counter = Arc::new(AtomicUsize::new(1));
        let old = Some((counter.clone(), 1));
        let waiter = tokio::spawn({
            let semaphore = semaphore.clone();
            async move { acquire_decode_permit(semaphore, &old).await }
        });
        tokio::task::yield_now().await;
        counter.store(2, Ordering::SeqCst);
        drop(held);
        assert!(waiter.await.unwrap().is_err());
        let current = Some((counter.clone(), 2));
        let permit = acquire_decode_permit(semaphore.clone(), &current)
            .await
            .unwrap();
        counter.store(3, Ordering::SeqCst);
        assert!(ensure_current(&current).is_err());
        drop(permit);
        assert_eq!(semaphore.available_permits(), 1);
        assert!(ensure_current(&None).is_ok());
    }
}
