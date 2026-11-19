//! Requests as data. Whatever reads the network here is a [`Task`]: it says
//! which request it needs next ([`Next::Ask`]) and is fed the answer, until
//! it is done. A task does no I/O itself, so the same task runs natively
//! (`native::Native`), in a web page (through a CORS proxy,
//! asynchronously) and in tests (canned answers).

/// A request a task needs answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Get(String),
    /// A POST of a JSON body (the MEGA API).
    Post {
        url: String,
        body: Vec<u8>,
    },
}

impl Request {
    /// Where it goes.
    pub fn url(&self) -> &str {
        match self {
            Request::Get(u) | Request::Post { url: u, .. } => u,
        }
    }
}

/// The body of a successful answer, or why there is none.
pub type Answer = Result<Vec<u8>, NetError>;

/// Why a request or a download gave nothing. Its text (`Display`) is what
/// a person reads ("HTTP 404", "cancelled").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetError {
    /// The server answered with this HTTP status.
    Status(u16),
    /// No data came for this many seconds.
    Stalled(u64),
    /// Its [`Progress`](crate::progress::Progress) was cancelled.
    Cancelled,
    /// A host says the file is no more (MEGA's -9 / -2): its words.
    Gone(String),
    /// The service refused BMSeeker (a status it
    /// [`refuses`](crate::refusal::refuses) with, a browser check) now or
    /// earlier in the run: it is not asked again ([`crate::refusal`]).
    Refused(crate::refusal::Refusal),
    /// Anything else (the connection, a host's own message), as said.
    Other(String),
}

impl NetError {
    /// The server busy (503) or its gateway failing (502, 504): it may
    /// pass. (A 429 is a refusal: [`NetError::Refused`].)
    pub(crate) fn passing(&self) -> bool {
        matches!(self, NetError::Status(502..=504))
    }

    /// The file is no more: 404 / 410, or a host's word for it.
    pub fn gone(&self) -> bool {
        matches!(self, NetError::Status(404 | 410) | NetError::Gone(_))
    }

    /// What it is, for a program to read (its text is a person's):
    /// `status`, `stalled`, `cancelled`, `gone`, `refused`, `other`.
    pub fn code(&self) -> &'static str {
        match self {
            NetError::Status(_) => "status",
            NetError::Refused(_) => "refused",
            NetError::Stalled(_) => "stalled",
            NetError::Cancelled => "cancelled",
            NetError::Gone(_) => "gone",
            NetError::Other(_) => "other",
        }
    }
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NetError::Status(n) => write!(f, "HTTP {n}"),
            NetError::Stalled(s) => write!(f, "no data for {s} s"),
            NetError::Cancelled => f.write_str(crate::progress::CANCELLED),
            NetError::Refused(r) => r.fmt(f),
            NetError::Gone(e) | NetError::Other(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for NetError {}

impl From<String> for NetError {
    fn from(e: String) -> NetError {
        NetError::Other(e)
    }
}

impl From<&str> for NetError {
    fn from(e: &str) -> NetError {
        NetError::Other(e.into())
    }
}

impl From<NetError> for String {
    fn from(e: NetError) -> String {
        e.to_string()
    }
}

/// What a task needs next.
#[derive(Debug, PartialEq, Eq)]
pub enum Next<T> {
    Ask(Request),
    Done(T),
}

/// A computation that needs requests answered.
pub trait Task: Send {
    type Output;
    /// The first step.
    fn start(&mut self) -> Next<Self::Output>;
    /// The answer to the last [`Next::Ask`]: the next step.
    fn feed(&mut self, answer: Answer) -> Next<Self::Output>;
}

/// A task of any kind with this output.
pub(crate) type BoxTask<T> = Box<dyn Task<Output = T>>;

impl<T: Task + ?Sized> Task for &mut T {
    type Output = T::Output;
    fn start(&mut self) -> Next<T::Output> {
        (**self).start()
    }
    fn feed(&mut self, answer: Answer) -> Next<T::Output> {
        (**self).feed(answer)
    }
}

impl<T> Task for BoxTask<T> {
    type Output = T;
    fn start(&mut self) -> Next<T> {
        (**self).start()
    }
    fn feed(&mut self, answer: Answer) -> Next<T> {
        (**self).feed(answer)
    }
}

/// A task that is done at once.
pub(crate) struct Ready<T>(Option<T>);

impl<T: Send> Ready<T> {
    pub(crate) fn boxed(value: T) -> BoxTask<T>
    where
        T: 'static,
    {
        Box::new(Ready(Some(value)))
    }
}

impl<T: Send> Task for Ready<T> {
    type Output = T;
    fn start(&mut self) -> Next<T> {
        Next::Done(self.0.take().expect("a Ready task starts once"))
    }
    fn feed(&mut self, _: Answer) -> Next<T> {
        self.start()
    }
}

/// One GET, then what `then` makes of its answer.
pub(crate) fn get_then<T, F>(url: String, then: F) -> BoxTask<T>
where
    T: Send + 'static,
    F: FnOnce(Answer) -> T + Send + 'static,
{
    struct Then<F> {
        url: String,
        then: Option<F>,
    }
    impl<T, F: FnOnce(Answer) -> T + Send> Task for Then<F> {
        type Output = T;
        fn start(&mut self) -> Next<T> {
            Next::Ask(Request::Get(self.url.clone()))
        }
        fn feed(&mut self, answer: Answer) -> Next<T> {
            match self.then.take() {
                Some(f) => Next::Done(f(answer)),
                // (fed twice: asks again, as at the start)
                None => self.start(),
            }
        }
    }
    Box::new(Then {
        url,
        then: Some(then),
    })
}

/// A run's answers to GETs asked already, for the tasks it
/// [wraps](Memo::wrap) (BMS SEARCH's `/patterns/<md5>` for each source
/// that asks it, a Drive folder's listing for each file in it): an answer
/// it has is given again, not asked again. A success and a "not there"
/// (404, 410) are kept, up to [`Memo::CAP`] bytes in all.
#[derive(Default)]
pub(crate) struct Memo(std::sync::Mutex<(usize, std::collections::HashMap<String, Answer>)>);

impl Memo {
    const CAP: usize = 4 << 20;

    /// `task`, its GETs answered from (and kept in) the memo.
    pub(crate) fn wrap<T: Task + 'static>(
        self: &std::sync::Arc<Self>,
        task: T,
    ) -> BoxTask<T::Output> {
        Box::new(Memoed {
            task,
            memo: self.clone(),
            asked: None,
        })
    }

    fn get(&self, url: &str) -> Option<Answer> {
        self.0.lock().unwrap().1.get(url).cloned()
    }

    fn put(&self, url: String, answer: &Answer) {
        let size = match answer {
            Ok(b) => b.len(),
            Err(NetError::Status(404 | 410)) => 0,
            Err(_) => return,
        };
        let mut m = self.0.lock().unwrap();
        if m.0 + size <= Self::CAP {
            m.0 += size;
            m.1.insert(url, answer.clone());
        }
    }
}

/// A task whose GETs a [`Memo`] answers when it can.
struct Memoed<T> {
    task: T,
    memo: std::sync::Arc<Memo>,
    /// The GET asked of the network, to keep its answer.
    asked: Option<String>,
}

impl<T: Task> Memoed<T> {
    /// `next`, the GETs the memo has answered.
    fn answered(&mut self, mut next: Next<T::Output>) -> Next<T::Output> {
        while let Next::Ask(Request::Get(url)) = &next {
            match self.memo.get(url) {
                Some(a) => next = self.task.feed(a),
                None => {
                    self.asked = Some(url.clone());
                    return next;
                }
            }
        }
        next
    }
}

impl<T: Task> Task for Memoed<T> {
    type Output = T::Output;
    fn start(&mut self) -> Next<T::Output> {
        self.asked = None;
        let next = self.task.start();
        self.answered(next)
    }
    fn feed(&mut self, answer: Answer) -> Next<T::Output> {
        if let Some(url) = self.asked.take() {
            self.memo.put(url, &answer);
        }
        let next = self.task.feed(answer);
        self.answered(next)
    }
}

/// What answers requests (in tests).
#[cfg(test)]
pub(crate) trait Http {
    fn send(&self, request: &Request) -> Answer;
}

/// Run `task` to its end, asking `http`.
#[cfg(test)]
pub(crate) fn run<T: Task + ?Sized>(task: &mut T, http: &dyn Http) -> T::Output {
    let mut next = task.start();
    loop {
        match next {
            Next::Ask(r) => next = task.feed(http.send(&r)),
            Next::Done(out) => return out,
        }
    }
}

/// Canned answers by URL, for tests.
#[cfg(test)]
pub(crate) struct Canned(pub Vec<(String, Answer)>);

#[cfg(test)]
impl Http for Canned {
    fn send(&self, r: &Request) -> Answer {
        self.0
            .iter()
            .find(|(u, _)| u == r.url())
            .map(|(_, a)| a.clone())
            .unwrap_or_else(|| Err(format!("no answer for {}", r.url()).into()))
    }
}

/// `http`'s answers, the URLs asked noted (in tests).
#[cfg(test)]
pub(crate) struct Asked<H>(pub H, pub std::cell::RefCell<Vec<String>>);

#[cfg(test)]
impl<H: Http> Asked<H> {
    pub(crate) fn new(http: H) -> Asked<H> {
        Asked(http, Default::default())
    }

    /// How often `url` was asked.
    pub(crate) fn times(&self, url: &str) -> usize {
        self.1.borrow().iter().filter(|u| *u == url).count()
    }
}

#[cfg(test)]
impl<H: Http> Http for Asked<H> {
    fn send(&self, r: &Request) -> Answer {
        self.1.borrow_mut().push(r.url().to_string());
        self.0.send(r)
    }
}

/// `s` percent-encoded for a URL's query.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The file name in a `Content-Disposition` header (`filename*=UTF-8''…`
/// or `filename="…"`; UTF-8 sent as Latin-1 read back as UTF-8).
pub fn disposition_name(h: &str) -> Option<String> {
    let lower = h.to_ascii_lowercase();
    let value = |key: &str| {
        let at = lower.find(key)? + key.len();
        let v = h[at..].split(';').next()?.trim().trim_matches('"');
        (!v.is_empty()).then(|| v.to_string())
    };
    let name = if let Some(v) = value("filename*=") {
        let v = v
            .split_once("''")
            .map_or(v.as_str(), |(_, n)| n)
            .to_string();
        percent_decode(&v)
    } else {
        let v = value("filename=")?;
        let latin1: Option<Vec<u8>> = v.chars().map(|c| u8::try_from(c as u32).ok()).collect();
        latin1.and_then(|b| String::from_utf8(b).ok()).unwrap_or(v)
    };
    crate::text::safe_file_name(&name)
}

/// `%XX` decoded (invalid UTF-8 replaced).
pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A memo answers a GET it has (a success, a 404), not one that failed
    /// otherwise, and only for the tasks it wraps.
    #[test]
    fn a_memo() {
        let memo = std::sync::Arc::new(Memo::default());
        let http = Asked::new(Canned(vec![
            ("https://a.test/ok".into(), Ok(b"1".to_vec())),
            ("https://a.test/gone".into(), Err(NetError::Status(404))),
            ("https://a.test/busy".into(), Err(NetError::Status(500))),
        ]));
        for _ in 0..2 {
            for u in [
                "https://a.test/ok",
                "https://a.test/gone",
                "https://a.test/busy",
            ] {
                let got = run(&mut memo.wrap(get_then(u.to_string(), |a| a)), &http);
                assert_eq!(got, http.0.send(&Request::Get(u.into())));
            }
        }
        assert_eq!(http.times("https://a.test/ok"), 1);
        assert_eq!(http.times("https://a.test/gone"), 1);
        assert_eq!(http.times("https://a.test/busy"), 2);
        let _ = run(&mut get_then("https://a.test/ok".into(), |a| a), &http);
        assert_eq!(http.times("https://a.test/ok"), 2);
    }

    #[test]
    fn file_names() {
        assert_eq!(
            disposition_name("attachment; filename=\"a b.zip\"").as_deref(),
            Some("a b.zip")
        );
        assert_eq!(
            disposition_name("attachment; filename=\"x.zip\"; filename*=UTF-8''%E6%9B%B2.zip")
                .as_deref(),
            Some("曲.zip")
        );
        // UTF-8 bytes sent as Latin-1
        let latin1: String = "曲.rar".bytes().map(|b| b as char).collect();
        assert_eq!(
            disposition_name(&format!("attachment; filename=\"{latin1}\"")).as_deref(),
            Some("曲.rar")
        );
    }

    /// Asks for two pages and gives their lengths.
    struct Two(Vec<usize>);
    impl Task for Two {
        type Output = Vec<usize>;
        fn start(&mut self) -> Next<Vec<usize>> {
            Next::Ask(Request::Get("a".into()))
        }
        fn feed(&mut self, a: Answer) -> Next<Vec<usize>> {
            self.0.push(a.map_or(0, |b| b.len()));
            if self.0.len() == 1 {
                Next::Ask(Request::Get("b".into()))
            } else {
                Next::Done(std::mem::take(&mut self.0))
            }
        }
    }

    #[test]
    fn runs() {
        let http = Canned(vec![
            ("a".into(), Ok(b"12".to_vec())),
            ("b".into(), Err(NetError::Status(404))),
        ]);
        assert_eq!(run(&mut Two(Vec::new()), &http), [2, 0]);
        let mut boxed: BoxTask<Vec<usize>> = Box::new(Two(Vec::new()));
        assert_eq!(run(&mut boxed, &http), [2, 0]);
        assert_eq!(run(&mut Ready::boxed(7), &http), 7);
        assert_eq!(url_encode("a b/c"), "a%20b%2Fc");
        assert_eq!(percent_decode("%E6%9B%B2%2"), "曲%2");
    }

    #[test]
    fn errors() {
        // the text a person reads stays what it was
        assert_eq!(NetError::Status(404).to_string(), "HTTP 404");
        assert_eq!(NetError::Cancelled.to_string(), "cancelled");
        assert_eq!(NetError::Stalled(60).to_string(), "no data for 60 s");
        assert!(NetError::Status(503).passing() && !NetError::Status(500).passing());
        // a refusal is not tried again
        assert!(!NetError::Status(429).passing());
        assert!(!NetError::Other("HTTP 503".into()).passing());
        assert!(NetError::Status(410).gone() && NetError::Gone("x".into()).gone());
        assert!(!NetError::Status(403).gone());
    }
}
