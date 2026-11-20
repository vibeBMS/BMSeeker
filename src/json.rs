//! Another party's JSON read into the few fields a caller looks at, the
//! rest passed by as it is read: a tree of every value (`serde_json::Value`)
//! costs some thirty times the answer's size. What is of another type than
//! the one looked for is as if it were not there, as a tree's `get` and
//! `as_str` had it. Each is a field's `deserialize_with`.

use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::marker::PhantomData;

/// What is made of one JSON value: of the type looked for, or `none` of
/// any other (read through).
trait Read<'de>: Sized {
    type Out;
    fn none(self) -> Self::Out;
    fn text(self, _: String) -> Self::Out {
        self.none()
    }
    fn number(self, _: u64) -> Self::Out {
        self.none()
    }
    fn list<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Out, A::Error> {
        while a.next_element::<IgnoredAny>()?.is_some() {}
        Ok(self.none())
    }
    fn object<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Out, A::Error> {
        while a.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(self.none())
    }
}

struct Any<R>(R);

impl<'de, R: Read<'de>> Visitor<'de> for Any<R> {
    type Value = R::Out;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("any value")
    }
    fn visit_str<E>(self, v: &str) -> Result<R::Out, E> {
        Ok(self.0.text(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<R::Out, E> {
        Ok(self.0.text(v))
    }
    fn visit_u64<E>(self, v: u64) -> Result<R::Out, E> {
        Ok(self.0.number(v))
    }
    fn visit_i64<E>(self, _: i64) -> Result<R::Out, E> {
        Ok(self.0.none())
    }
    fn visit_f64<E>(self, _: f64) -> Result<R::Out, E> {
        Ok(self.0.none())
    }
    fn visit_bool<E>(self, _: bool) -> Result<R::Out, E> {
        Ok(self.0.none())
    }
    fn visit_unit<E>(self) -> Result<R::Out, E> {
        Ok(self.0.none())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, a: A) -> Result<R::Out, A::Error> {
        self.0.list(a)
    }
    fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<R::Out, A::Error> {
        self.0.object(a)
    }
}

/// A string (else empty).
pub(crate) fn text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct R;
    impl Read<'_> for R {
        type Out = String;
        fn none(self) -> String {
            String::new()
        }
        fn text(self, v: String) -> String {
            v
        }
    }
    d.deserialize_any(Any(R))
}

/// A whole number that is not negative.
pub(crate) fn number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    struct R;
    impl Read<'_> for R {
        type Out = Option<u64>;
        fn none(self) -> Option<u64> {
            None
        }
        fn number(self, v: u64) -> Option<u64> {
            Some(v)
        }
    }
    d.deserialize_any(Any(R))
}

/// An object as `T` (else `T`'s default).
pub(crate) fn object<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Object::deserialize(d)?.0.unwrap_or_default())
}

/// A list's objects as `T`s (what else it holds passed by; no list: none).
pub(crate) fn objects<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct R<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Read<'de> for R<T> {
        type Out = Vec<T>;
        fn none(self) -> Vec<T> {
            Vec::new()
        }
        fn list<A: SeqAccess<'de>>(self, mut a: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::new();
            while let Some(o) = a.next_element::<Object<T>>()? {
                out.extend(o.0);
            }
            Ok(out)
        }
    }
    d.deserialize_any(Any(R(PhantomData)))
}

/// A list's strings (what else it holds passed by; no list: none).
pub(crate) fn texts<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    struct Text(Option<String>);
    impl<'de> Deserialize<'de> for Text {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Text, D::Error> {
            struct R;
            impl Read<'_> for R {
                type Out = Text;
                fn none(self) -> Text {
                    Text(None)
                }
                fn text(self, v: String) -> Text {
                    Text(Some(v))
                }
            }
            d.deserialize_any(Any(R))
        }
    }
    struct R;
    impl<'de> Read<'de> for R {
        type Out = Vec<String>;
        fn none(self) -> Vec<String> {
            Vec::new()
        }
        fn list<A: SeqAccess<'de>>(self, mut a: A) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(t) = a.next_element::<Text>()? {
                out.extend(t.0);
            }
            Ok(out)
        }
    }
    d.deserialize_any(Any(R))
}

/// An object as `T` (None: another value).
pub(crate) struct Object<T>(pub(crate) Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Object<T>, D::Error> {
        struct R<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Read<'de> for R<T> {
            type Out = Object<T>;
            fn none(self) -> Object<T> {
                Object(None)
            }
            fn object<A: MapAccess<'de>>(self, a: A) -> Result<Object<T>, A::Error> {
                T::deserialize(MapAccessDeserializer::new(a)).map(|t| Object(Some(t)))
            }
        }
        d.deserialize_any(Any(R(PhantomData)))
    }
}

/// `body` as the object `T` (`T`'s default: no object, or no JSON).
pub(crate) fn object_of<T: serde::de::DeserializeOwned + Default>(body: &[u8]) -> T {
    let read = serde_json::from_slice::<Object<T>>(body);
    read.ok().and_then(|o| o.0).unwrap_or_default()
}

/// `body` as a list's objects (`T`s; none: no list, or no JSON).
pub(crate) fn objects_of<T: serde::de::DeserializeOwned>(body: &[u8]) -> Vec<T> {
    let mut d = serde_json::Deserializer::from_slice(body);
    let read = objects(&mut d).and_then(|out| d.end().map(|()| out));
    read.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize, Default, Debug, PartialEq)]
    #[serde(default)]
    struct Inner {
        #[serde(deserialize_with = "text")]
        id: String,
    }

    #[derive(Deserialize, Default, Debug, PartialEq)]
    #[serde(default)]
    struct Work {
        #[serde(deserialize_with = "text")]
        title: String,
        #[serde(deserialize_with = "number")]
        num: Option<u64>,
        #[serde(deserialize_with = "object")]
        bms: Inner,
        #[serde(deserialize_with = "objects")]
        files: Vec<Inner>,
        #[serde(deserialize_with = "texts")]
        links: Vec<String>,
    }

    /// The fields looked for, of the types looked for; anything else is
    /// as if it were not there, wherever it is.
    #[test]
    fn the_fields_looked_for() {
        let w: Work = object_of(
            br#"{"title":"a","num":7,"x":[{"y":[1,{}]}],"bms":{"id":"b","z":null},
                "files":[1,{"id":"c"},"s",[{"id":"no"}],{"id":5},null],"links":["l",2,["m"],"n"]}"#,
        );
        assert_eq!(w.title, "a");
        assert_eq!(w.num, Some(7));
        assert_eq!(w.bms.id, "b");
        let ids: Vec<&str> = w.files.iter().map(|f| &*f.id).collect();
        assert_eq!(ids, ["c", ""]);
        assert_eq!(w.links, ["l", "n"]);
        let odd: Work = object_of(
            br#"{"title":1,"num":"7","bms":[{"id":"b"}],"files":{"id":"c"},"links":"l"}"#,
        );
        assert_eq!(odd, Work::default());
        let odd: Work = object_of(br#"{"title":null,"num":-1,"bms":null,"files":null}"#);
        assert_eq!(odd, Work::default());
        assert_eq!(object_of::<Work>(b"[1]"), Work::default());
        assert_eq!(object_of::<Work>(b"{"), Work::default());
        let list: Vec<Inner> = objects_of(br#"[{"id":"a"},3,{"id":"b"}]"#);
        assert_eq!(list.len(), 2);
        assert!(objects_of::<Inner>(br#"{"id":"a"}"#).is_empty());
        assert!(objects_of::<Inner>(br#"[{"id":"a"}"#).is_empty());
    }
}
