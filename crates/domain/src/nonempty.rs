//! A collection that cannot be empty. Parsed once at the boundary; never asserted later.

/// At least one `T`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "Vec<T>", into = "Vec<T>"))]
pub struct NonEmpty<T: Clone> {
    head: T,
    tail: Vec<T>,
}

/// The input had no elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected at least one element")]
pub struct EmptyError;

impl<T: Clone> NonEmpty<T> {
    #[must_use]
    pub fn new(head: T, tail: Vec<T>) -> Self {
        Self { head, tail }
    }

    #[must_use]
    pub fn singleton(head: T) -> Self {
        Self {
            head,
            tail: Vec::new(),
        }
    }

    #[must_use]
    pub const fn first(&self) -> &T {
        &self.head
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tail.len().saturating_add(1)
    }

    /// Always false; present so callers reading `len()` are not surprised.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        core::iter::once(&self.head).chain(self.tail.iter())
    }

    /// Apply `f` to every element, keeping non-emptiness.
    pub fn map<U: Clone, F: FnMut(T) -> U>(self, mut f: F) -> NonEmpty<U> {
        NonEmpty {
            head: f(self.head),
            tail: self.tail.into_iter().map(f).collect(),
        }
    }

    /// Apply a fallible `f` to every element; the first error wins.
    ///
    /// # Errors
    /// The first error `f` returns.
    pub fn try_map<U: Clone, E, F: FnMut(T) -> Result<U, E>>(
        self,
        mut f: F,
    ) -> Result<NonEmpty<U>, E> {
        Ok(NonEmpty {
            head: f(self.head)?,
            tail: self.tail.into_iter().map(f).collect::<Result<_, _>>()?,
        })
    }
}

impl<T: Clone> TryFrom<Vec<T>> for NonEmpty<T> {
    type Error = EmptyError;

    fn try_from(mut v: Vec<T>) -> Result<Self, Self::Error> {
        if v.is_empty() {
            return Err(EmptyError);
        }
        let head = v.remove(0);
        Ok(Self { head, tail: v })
    }
}

impl<T: Clone> From<NonEmpty<T>> for Vec<T> {
    fn from(n: NonEmpty<T>) -> Self {
        let mut v = Vec::with_capacity(n.len());
        v.push(n.head);
        v.extend(n.tail);
        v
    }
}

impl<'a, T: Clone> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = core::iter::Chain<core::iter::Once<&'a T>, core::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(&self.head).chain(self.tail.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_empty_rejected() {
        assert_eq!(NonEmpty::<u8>::try_from(vec![]), Err(EmptyError));
        let n = NonEmpty::try_from(vec![1, 2, 3]).unwrap();
        assert_eq!((n.len(), *n.first(), n.is_empty()), (3, 1, false));
        assert_eq!(Vec::from(n.clone()), vec![1, 2, 3]);
        assert_eq!(n.iter().sum::<u8>(), 6);
        assert_eq!(Vec::from(n.clone().map(|x| x * 2)), vec![2, 4, 6]);
        assert!(
            n.clone()
                .try_map(|x| if x == 2 { Err("two") } else { Ok(x) })
                .is_err()
        );
        assert_eq!(NonEmpty::singleton(9).len(), 1);
        assert_eq!(NonEmpty::new(1, vec![2]).len(), 2);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_is_a_plain_array() {
        let n: NonEmpty<String> = serde_json::from_str(r#"["a","b"]"#).unwrap();
        assert_eq!(serde_json::to_string(&n).unwrap(), r#"["a","b"]"#);
        assert!(serde_json::from_str::<NonEmpty<String>>("[]").is_err());
    }
}
