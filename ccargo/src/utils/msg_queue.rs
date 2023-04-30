/// This struct abstracts the logic required to print messages to the 
/// standard output interactively in a particular order from multiple threads.
/// 
/// If you are compiling multiple libraries in parallel, each of which is
/// itself compiling multiple files in parallel, then naively combining the
/// compiler outputs will make it much harder to understand warnings/errors,
/// as there will be unrelated warnings/errors being printed one after another,
/// possibly interleaved if the implementation is particularly naive. 
///
/// One solution is to collect the outputs seperately per parallel process,
/// and combine them afterwards, but this results in a loss in percieved interactivity
/// as you lose the ability to see how quickly each file is compiled, which is a 'nonsense' 
/// feature that has no quantitative meaning but makes compiling more pleasant for some (including me).
/// 
/// We want to make the output as structured and interactive as possible, while maintaining our ability to 
/// collect the output in multiple processes.
/// 
/// The idea is to split the output into `buckets`.
/// The output is then cached and forwarded based on the order of these buckets.
/// The caching/forwarding is done at message generation time, so it is as interactive as possible.

use std::cell::{RefCell, RefMut};
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};


pub struct MsgQueue<W: Write> {
    inner: Arc<Inner<W>>,
}

pub struct MsgWriter<W: Write> {
    queue: MsgQueue<W>,
    // We need an Arc<usize> so that we can keep track of the ref count of
    // this writer separately from the ref count of the queue, so if a writer 
    // is cloned, only one writer (the last) will flush the cached messages
    index: Arc<usize>,
    // Filesystem cache used to save outputs
    cache: Option<FileCache>,
}

struct Inner<W: Write> {
    output: RefCell<W>,
    cached: Vec<RefCell<Vec<u8>>>,
    done: Vec<AtomicUsize>,
    count: AtomicUsize,
    current: AtomicUsize,
    capacity: usize,
}

#[derive(Clone)]
struct FileCache {
    path: PathBuf,
    data: RefCell<Vec<u8>>,
}

impl MsgQueue<Vec<u8>> {
    /// Create a new MsgQueue that outputs to a Vec
    pub fn buffer(capacity: usize) -> Self {
        Self::new(capacity, Vec::new())
    }
}

impl<W: Write> MsgQueue<W> {
    /// Create a new MsgQueue that outputs to the given `io::Write`
    pub fn new(capacity: usize, output: W) -> Self {
        let mut inner = Inner {
            output: RefCell::new(output),
            count: AtomicUsize::new(0),
            current: AtomicUsize::new(0),
            cached: Vec::new(),
            done: Vec::new(),
            capacity,
        };
        inner.cached.resize_with(capacity, Default::default);
        inner.done.resize_with(capacity, || AtomicUsize::new(0));
        Self { inner: Arc::new(inner) }
    }

    /// Resize the queue a writer for writing messages to the queue
    pub fn resize(&mut self, capacity: usize) {
        if let Some(me) = Arc::get_mut(&mut self.inner) {
            me.cached.resize_with(capacity, Default::default);
            me.done.resize_with(capacity, || AtomicUsize::new(0));
            me.capacity = capacity;
        } else {
            panic!("Resizing MsgQueue with multiple alive references")
        }
    }

    /// Create a writer for writing messages to the queue
    pub fn writer(&self) -> MsgWriter<W> {
        MsgWriter{ 
            queue: self.clone(),
            index: Arc::new(self.inner.add()),
            cache: None,
        }
    }
    
    /// Destroy the MsgQueue and return the output
    pub fn output(self) -> W {
        Arc::try_unwrap(self.inner)
            .expect("Cannot get data from MsgQueue when multiple references are alive")
            .output
            .into_inner()            
    }
}

impl<W: Write> MsgWriter<W> {
    /// Create a nested message queue that will respect this writer's order
    /// but can create it's own writers that respect a different order
    /// NOTE: all writers created from this message queue must be destroyed
    /// before the current writer is destroyed, otherwise some messages may
    /// not be flushed correctly
    pub fn nested(&self, capacity: usize) -> MsgQueue<MsgWriter<W>> {
        MsgQueue::new(capacity, self.clone())
    }

    // Push a message to the queue based on the order of this writer
    pub fn push(&self, buf: &[u8]) -> io::Result<()> {
        if let Some(mut c) = self.cache_mut() { c.write_all(buf)?; }
        self.write_mut().write_all(buf)
    }

    // Set the path where this writer will write its cached output on destruction
    pub fn set_cache_path<P: AsRef<Path>>(&mut self, path: P) {
        self.cache = Some(FileCache{path: PathBuf::from(path.as_ref()), data: RefCell::default()})
    }

    // Load output from the given cache path and write it to this writer
    pub fn load_cache_from_path<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        if let Ok(content) = std::fs::read(path) {
            self.push(&content)?;
        }
        Ok(())
    }

    fn write_mut(&self) -> RefMut<dyn Write> {
        self.queue.inner.write_mut(*self.index)
    }

    fn cache_mut(&self) -> Option<RefMut<Vec<u8>>> {
        self.cache.as_ref().map(|v| v.data.borrow_mut())
    }
    
    fn save_if_cached(&mut self) {
        if let Some(cache) = self.cache.take() {
            if cache.data.borrow().is_empty() {
                drop(std::fs::remove_file(&cache.path));
                return;
            }
            std::fs::write(&cache.path, cache.data.into_inner())
                .expect(&format!("Failed to write cached outputs to `{}`", cache.path.display()));
        }
    }
}

impl<W: Write> Write for MsgWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(mut c) = self.cache_mut() { c.write(buf)?; }
        self.write_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write_mut().flush()
    }
}

impl<W: Write> Drop for MsgWriter<W> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.index) == 1 {
            self.queue.inner.finish(*self.index);
            self.save_if_cached();
        }
    }
}

// We manually ensure the thread safely of MsgQueue
unsafe impl<W: Write> Send for MsgQueue<W> {}
unsafe impl<W: Write> Sync for MsgQueue<W> {}

impl<W: Write> Clone for MsgQueue<W> {
    fn clone(&self) -> Self {
        Self{inner: self.inner.clone()}
    }
}

impl<W: Write> Clone for MsgWriter<W> {
    fn clone(&self) -> Self {
        Self{queue: self.queue.clone(), index: self.index.clone(), cache: self.cache.clone()}
    }
}

impl<W: Write> std::fmt::Debug for Inner<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("...", &"...")
            .finish()
    }
}

impl<W: Write> Inner<W> {
    fn add(&self) -> usize {
        let index = self.count.fetch_add(1, Ordering::SeqCst);
        assert!(
            index < self.capacity, 
            "Requested too many writers ({}), try increasing the capacity", 
            self.capacity
        );
        index
    }

    fn write_mut(&self, index: usize) -> RefMut<dyn Write> {
        if self.is_live(index) {
            self.output.borrow_mut()
        } else {
            self.cached[index].borrow_mut()
        }
    }

    fn finish(&self, index: usize) {
        self.mark_finished(index);
        let mut cur = self.current.load(Ordering::SeqCst);
        if index == cur {
            let end = self.count.load(Ordering::SeqCst);
            while cur < end && self.is_finished(cur) {
                self.flush_cached(cur);
                cur += 1;
            }
            self.current.store(cur, Ordering::SeqCst);
        }
    }

    fn is_live(&self, index: usize) -> bool {
        index == self.current.load(Ordering::SeqCst)
    }

    fn is_finished(&self, index: usize) -> bool {
        self.done[index].load(Ordering::SeqCst) != 0
    }

    fn mark_finished(&self, index: usize) {
        self.done[index].store(1, Ordering::SeqCst)
    }

    fn flush_cached(&self, index: usize) {
        let c = self.cached[index].borrow();
        if !c.is_empty() {
            self.output
                .borrow_mut()
                .write_all(c.as_slice())
                .unwrap();
        }
    }
}

/*
fn test_threaded(steps: usize) {
    use std::thread;

    let n: usize = thread::available_parallelism().unwrap().into();

    let q = MsgQueue::buffer(n);

    let mut handles = Vec::new();
    for i in 0..n {
        let w = q.writer();
        handles.push(thread::spawn(move || {
            let s = &[i as u8];
            for x in 0..steps {
                if (x % ((i + 1) * 100)) == 0 {
                    thread::sleep(std::time::Duration::from_nanos(1));
                }
                w.push(s).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    
    let out = q.output();
    for i in 0..n {
        let u = i as u8;
        let slice = &out[i*steps..(i+1)*steps];
        if let Some(pos) = slice.iter().position(|v| *v != u) {
            panic!(
                "Expected `{i}` but got `{}` in position {pos}", 
                slice[pos],
            );
        }
    }
}
*/
