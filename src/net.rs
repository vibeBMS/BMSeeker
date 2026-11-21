//! Requests as data. Whatever reads the network here is a [`Task`]: it says
//! which request it needs next ([`Next::Ask`]) and is fed the answer, until
//! it is done. A task does no I/O itself, so the same task runs natively
//! (`native::Native`), in a web page (through a CORS proxy,
//! asynchronously) and in tests (canned answers).

/// What a request fetches: how large its answer may be ([`Kind::limit`]).
/// The limits are some ten times the largest of each kind measured (a
/// table's page 5 KB, its header 13 KB, its data 3.4 MB; a venue's work
/// list 1.7 MB; an event's index 0.2 MB; the Stella list 2.5 MB), so a
/// small answer that decodes to a huge one is cut off where it is read,
/// not after it was parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An HTML page: a table's, a venue's, a host's.
    Page,
    /// A difficulty table's header.
    Header,
    /// A difficulty table's data.
    TableData,
    /// An API's answer, or a JSON file of its size (an event's index).
    Api,
    /// A folder's listing (Google Drive's, MEGA's) or a list of links.
    Listing,
}

impl Kind {
    /// The bytes its answer may have, decoded.
    pub const fn limit(self) -> usize {
        match self {
            Kind::Header => 1 << 20,
            Kind::Api => 2 << 20,
            Kind::Page => 8 << 20,
            Kind::Listing => 16 << 20,
            Kind::TableData => 32 << 20,
        }
    }
}

/// A request a task needs answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Get(String, Kind),
    /// A POST of a JSON body (the MEGA API).
    Post {
        url: String,
        body: Vec<u8>,
        kind: Kind,
    },
}

impl Request {
    /// Where it goes.
    pub fn url(&self) -> &str {
        match self {
            Request::Get(u, _) | Request::Post { url: u, .. } => u,
        }
    }

    /// What it fetches.
    pub fn kind(&self) -> Kind {
        match self {
            Request::Get(_, k) | Request::Post { kind: k, .. } => *k,
        }
    }

    /// The bytes its answer may have: a driver reads no more, and a larger
    /// answer is [`too_large`].
    pub fn limit(&self) -> usize {
        self.kind().limit()
    }
}

/// The failure of an answer larger than its request's `limit`.
pub fn too_large(limit: usize) -> NetError {
    NetError::Other(format!(
        "the answer is larger than {} MiB: not read",
        limit >> 20
    ))
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
    Refused(Refusal),
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

/// Why a service is not asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// It answered this HTTP status (401, 403, 429, 451).
    Status(u16),
    /// It answered this HTTP status (401, 403, 451) for
    /// [`STRIKES`](crate::refusal::STRIKES) files' addresses one after
    /// another, none of its answers coming between them.
    Files(u16),
    /// It answered with a browser check (a Cloudflare challenge page,
    /// Turnstile).
    Challenge,
    /// Its API said it limits BMSeeker's requests (MEGA's `-4`).
    RateLimited,
    /// It refused in an earlier run of this session (`--skip-host`).
    Earlier,
}

/// A service that refused, by its host
/// ([`service`](crate::refusal::service)): [`NetError::Refused`]'s payload,
/// so it is here; made and told in [`crate::refusal`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub host: String,
    pub why: Why,
}

impl Refusal {
    /// What it was, for a program: `status`, `files`, `challenge`,
    /// `rate_limited`, `earlier`.
    pub fn code(&self) -> &'static str {
        match self.why {
            Why::Status(_) => "status",
            Why::Files(_) => "files",
            Why::Challenge => "challenge",
            Why::RateLimited => "rate_limited",
            Why::Earlier => "earlier",
        }
    }

    /// The HTTP status it answered, if that was it.
    pub fn status(&self) -> Option<u16> {
        match self.why {
            Why::Status(n) | Why::Files(n) => Some(n),
            _ => None,
        }
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

/// One GET of an API's answer ([`Kind::Api`]), then what `then` makes of
/// it.
pub(crate) fn get_then<T, F>(url: String, then: F) -> BoxTask<T>
where
    T: Send + 'static,
    F: FnOnce(Answer) -> T + Send + 'static,
{
    get_as(url, Kind::Api, then)
}

/// One GET of `kind`, then what `then` makes of its answer.
pub(crate) fn get_as<T, F>(url: String, kind: Kind, then: F) -> BoxTask<T>
where
    T: Send + 'static,
    F: FnOnce(Answer) -> T + Send + 'static,
{
    struct Then<F> {
        url: String,
        kind: Kind,
        then: Option<F>,
    }
    impl<T, F: FnOnce(Answer) -> T + Send> Task for Then<F> {
        type Output = T;
        fn start(&mut self) -> Next<T> {
            Next::Ask(Request::Get(self.url.clone(), self.kind))
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
        kind,
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
        while let Next::Ask(Request::Get(url, _)) = &next {
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

/// The host name in `url` (or `url`, a host name): without the scheme,
/// user and port; as written.
pub(crate) fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    host.split(':').next().unwrap_or(host)
}

/// Whether `url` is written as every URL parser reads it: no backslash
/// (a browser's parser takes one for `/`, so what is before it is the
/// host there and a user's name here), no control character (dropped by a
/// browser's), no user name or password before the host. A link is
/// checked before any rule looks at its host ([`host`]), so the host the
/// rules see is the one the request goes to.
pub(crate) fn plainly_written(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !url.contains(|c: char| c == '\\' || c.is_control()) && !authority.contains('@')
}

/// Whether the host name `host` (an IPv6 address with or without its
/// brackets) is this machine or one of a network of its own: the name
/// `localhost` (and those under it), an address that is unspecified,
/// loopback, private, link-local, shared (100.64/10) or unique-local, or
/// a number that is no plain IPv4 address (`2130706433`, `0x7f.1`, which
/// a resolver reads as one); no name at all too.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn local_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']'));
    if let Ok(ip) = bare.unwrap_or(&host).parse() {
        return local_address(ip);
    }
    // (a name's last label has a letter; a number's is an address)
    let last = host.rsplit('.').next().unwrap_or("");
    let hex = last.strip_prefix("0x");
    last.is_empty()
        || last.bytes().all(|b| b.is_ascii_digit())
        || hex.is_some_and(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Whether `ip` is this machine's or a network of its own's
/// ([`local_host`]).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn local_address(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(a) => {
            let [x, y, ..] = a.octets();
            x == 0
                || a.is_loopback()
                || a.is_private()
                || a.is_link_local()
                || a.is_broadcast()
                || (x == 100 && y & 0xc0 == 64)
        }
        std::net::IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => local_address(std::net::IpAddr::V4(v4)),
            None => {
                a.is_unspecified()
                    || a.is_loopback()
                    || a.segments()[0] & 0xfe00 == 0xfc00
                    || a.segments()[0] & 0xffc0 == 0xfe80
            }
        },
    }
}

/// Whether `host` is `domain` or a name under it, in any case.
pub(crate) fn under(host: &str, domain: &str) -> bool {
    let (h, d) = (host.as_bytes(), domain.as_bytes());
    let Some(at) = h.len().checked_sub(d.len()) else {
        return false;
    };
    h[at..].eq_ignore_ascii_case(d) && (at == 0 || h[at - 1] == b'.')
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
                let asked = get_then(u.to_string(), |a| a);
                let got = run(&mut memo.wrap(asked), &http);
                assert_eq!(got, http.0.send(&Request::Get(u.into(), Kind::Api)));
            }
        }
        assert_eq!(http.times("https://a.test/ok"), 1);
        assert_eq!(http.times("https://a.test/gone"), 1);
        assert_eq!(http.times("https://a.test/busy"), 2);
        let again = get_then("https://a.test/ok".into(), |a| a);
        let _ = run(&mut { again }, &http);
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
            Next::Ask(Request::Get("a".into(), Kind::Page))
        }
        fn feed(&mut self, a: Answer) -> Next<Vec<usize>> {
            self.0.push(a.map_or(0, |b| b.len()));
            if self.0.len() == 1 {
                Next::Ask(Request::Get("b".into(), Kind::Page))
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
        assert_eq!(
            host("https://u@Www.Example.com:8080/a?b"),
            "Www.Example.com"
        );
        assert_eq!(host("example.com"), "example.com");
        assert!(under("dl.Dropbox.com", "dropbox.com") && under("dropbox.com", "dropbox.com"));
        assert!(!under("notdropbox.com", "dropbox.com") && !under("com", "dropbox.com"));
        assert_eq!(percent_decode("%E6%9B%B2%2"), "曲%2");
    }

    /// A link is taken as written only where every parser reads its host
    /// alike.
    #[test]
    fn links_plainly_written() {
        for u in [
            "https://www.dropbox.com/s/a/b.zip?dl=0",
            "http://example.com:8080/a b/@c\u{66f2}.zip#x@y",
            "https://[2001:db8::1]/a",
        ] {
            assert!(plainly_written(u), "{u}");
        }
        for u in [
            // (a browser's host: evil.tld; here: www.dropbox.com)
            "https://evil.tld\\@www.dropbox.com/x",
            "https://evil.tld/\\..\\x",
            "https://user@example.com/a",
            "https://user:pw@example.com/a",
            "https://exa\tmple.com/a",
            "https://example.com/a\nb",
            "https://example.com/\u{7f}",
        ] {
            assert!(!plainly_written(u), "{u:?}");
        }
    }

    /// The hosts that are this machine or a network of its own, by name
    /// and by address, however the address is written.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn local_hosts() {
        for h in [
            "localhost",
            "LocalHost.",
            "a.localhost",
            "127.0.0.1",
            "127.8.9.1",
            "0.0.0.0",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "255.255.255.255",
            "[::1]",
            "::1",
            "[::]",
            "[fe80::1]",
            "[fd00::1]",
            "[fc00::1]",
            "[::ffff:127.0.0.1]",
            "[::ffff:192.168.0.1]",
            // what a resolver reads as an address
            "2130706433",
            "0x7f000001",
            "127.1",
            "0x7f.0.0.1",
            "",
        ] {
            assert!(local_host(h), "{h}");
        }
        for h in [
            "example.com",
            "localhost.example.com",
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "192.169.1.1",
            "[2001:db8::1]",
            "[::ffff:8.8.8.8]",
            "1drv.ms",
            "a.0x.dev",
        ] {
            assert!(!local_host(h), "{h}");
        }
    }

    #[test]
    fn limits() {
        // the kinds in the order of their sizes: a header, an API's answer,
        // a page, a listing, a table's data
        let kinds = [
            Kind::Header,
            Kind::Api,
            Kind::Page,
            Kind::Listing,
            Kind::TableData,
        ];
        assert!(kinds.windows(2).all(|k| k[0].limit() < k[1].limit()));
        let post = Request::Post {
            url: "https://a.test/".into(),
            body: Vec::new(),
            kind: Kind::Listing,
        };
        assert_eq!(post.limit(), 16 << 20);
        assert_eq!(Request::Get("u".into(), Kind::Header).limit(), 1 << 20);
        assert_eq!(
            too_large(2 << 20).to_string(),
            "the answer is larger than 2 MiB: not read"
        );
    }

    #[test]
    fn errors() {
        // the text a person reads
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
