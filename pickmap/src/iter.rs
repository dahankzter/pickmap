/// Iterator over a map's entries in declaration order.
pub struct Iter<V: 'static> {
    keys: core::slice::Iter<'static, &'static str>,
    values: core::slice::Iter<'static, V>,
}

impl<V: 'static> Iter<V> {
    #[doc(hidden)]
    pub fn new(keys: &'static [&'static str], values: &'static [V]) -> Self {
        Iter { keys: keys.iter(), values: values.iter() }
    }
}

impl<V: 'static> Clone for Iter<V> {
    fn clone(&self) -> Self {
        Iter { keys: self.keys.clone(), values: self.values.clone() }
    }
}

impl<V: 'static> Iterator for Iter<V> {
    type Item = (&'static str, &'static V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some((*self.keys.next()?, self.values.next()?))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.keys.size_hint()
    }
}

impl<V: 'static> DoubleEndedIterator for Iter<V> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        Some((*self.keys.next_back()?, self.values.next_back()?))
    }
}

impl<V: 'static> ExactSizeIterator for Iter<V> {}
