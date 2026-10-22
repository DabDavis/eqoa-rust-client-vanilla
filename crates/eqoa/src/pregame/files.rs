//! The pregame's files off the disc, asked for by their paths on it
//! (`DATA2/SETUP.CSF`) and handed back whole once read: natively on a thread
//! of their own, so the window keeps drawing (the console draws its loading
//! screen meanwhile), in a page by the page's own reader (`set_reader`),
//! whose reads finish on later frames.

use std::cell::RefCell;
use std::rc::Rc;

/// The files of one request, in the order asked, or why they could not be read.
pub type Files = Result<Vec<(String, Vec<u8>)>, String>;

/// A request a page's reader answers into: filled on a later frame.
pub type Slot = Rc<RefCell<Option<Files>>>;

/// A page's reader: starts reading the paths and fills the slot it returns.
pub type Reader = Box<dyn Fn(&[String]) -> Slot>;

thread_local! {
    static READER: RefCell<Option<Reader>> = const { RefCell::new(None) };
}

/// Makes `r` the way the pregame's files are read (a page's).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_reader(r: Reader) {
    READER.with(|c| *c.borrow_mut() = Some(r));
}

/// Files being read.
pub enum Pending {
    Slot(Slot),
    #[cfg(not(target_arch = "wasm32"))]
    Thread(std::sync::mpsc::Receiver<Files>),
    /// in hand already (a page's refusal)
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Done(Option<Files>),
}

impl Pending {
    /// The files, once, when they have been read.
    pub fn poll(&mut self) -> Option<Files> {
        match self {
            Pending::Slot(s) => s.borrow_mut().take(),
            #[cfg(not(target_arch = "wasm32"))]
            Pending::Thread(rx) => match rx.try_recv() {
                Ok(f) => Some(f),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the reading thread stopped".into())),
            },
            Pending::Done(f) => f.take(),
        }
    }
}

/// Starts reading `paths` off the disc.
pub fn read(paths: &[&str]) -> Pending {
    let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    if let Some(s) = READER.with(|c| c.borrow().as_ref().map(|f| f(&paths))) {
        return Pending::Slot(s);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let iso = crate::disc_path(None);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let go = || -> Files {
                let mut d = iso::Iso::open(&iso).map_err(|e| format!("{iso}: {e}"))?;
                paths.iter().map(|p| d.read(p).map(|b| (p.clone(), b)).map_err(|e| format!("{p}: {e}"))).collect()
            };
            let _ = tx.send(go());
        });
        Pending::Thread(rx)
    }
    #[cfg(target_arch = "wasm32")]
    Pending::Done(Some(Err(format!("no reader for {paths:?}"))))
}
