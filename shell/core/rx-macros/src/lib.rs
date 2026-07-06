//! Small declarative macros for composing RxRust observables.
//!
//! These macros do not introduce a runtime abstraction. They expand to normal
//! RxRust operator chains and exist only to keep source composition concise.

/// Combines the latest values from multiple observables.
///
/// The macro expands to RxRust's binary `combine_latest` operator and returns a
/// flattened tuple. Use the `=>` form to map the tuple in one final `map`.
///
/// # Examples
///
/// ```rust
/// use rxrust::prelude::*;
/// use shell_rx_macros::combine_latest;
///
/// let combined = combine_latest!(Shared::<()>::of(1), Shared::<()>::of("a"));
/// let mapped = combine_latest!(
///     Shared::<()>::of(1),
///     Shared::<()>::of(2),
///     Shared::<()>::of(3) => |(a, b, c)| a + b + c,
/// );
/// ```
#[macro_export]
macro_rules! combine_latest {
    ($($source:expr),+ => $mapper:expr $(,)?) => {
        $crate::combine_latest!($($source),+).map($mapper)
    };
    ($source:expr $(,)?) => {
        compile_error!("combine_latest! requires at least two observables")
    };
    ($a:expr, $b:expr $(,)?) => {
        ($a).combine_latest($b, |a, b| (a, b))
    };
    ($a:expr, $b:expr, $c:expr $(,)?) => {
        $crate::combine_latest!($a, $b).combine_latest($c, |(a, b), c| (a, b, c))
    };
    ($a:expr, $b:expr, $c:expr, $d:expr $(,)?) => {
        $crate::combine_latest!($a, $b, $c).combine_latest($d, |(a, b, c), d| {
            (a, b, c, d)
        })
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr $(,)?) => {
        $crate::combine_latest!($a, $b, $c, $d).combine_latest($e, |(a, b, c, d), e| {
            (a, b, c, d, e)
        })
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr $(,)?) => {
        $crate::combine_latest!($a, $b, $c, $d, $e).combine_latest($f, |(a, b, c, d, e), f| {
            (a, b, c, d, e, f)
        })
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr, $g:expr $(,)?) => {
        $crate::combine_latest!($a, $b, $c, $d, $e, $f)
            .combine_latest($g, |(a, b, c, d, e, f), g| {
                (a, b, c, d, e, f, g)
            })
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr, $g:expr, $h:expr $(,)?) => {
        $crate::combine_latest!($a, $b, $c, $d, $e, $f, $g)
            .combine_latest($h, |(a, b, c, d, e, f, g), h| {
                (a, b, c, d, e, f, g, h)
            })
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr, $g:expr, $h:expr, $i:expr $(,)?) => {
        $crate::combine_latest!($a, $b, $c, $d, $e, $f, $g, $h)
            .combine_latest($i, |(a, b, c, d, e, f, g, h), i| {
                (a, b, c, d, e, f, g, h, i)
            })
    };
}

#[cfg(test)]
mod tests {
    use rxrust::prelude::*;

    #[test]
    fn combines_latest_values_into_flat_tuple() {
        let mut value = None;

        crate::combine_latest!(
            Shared::<()>::of(1),
            Shared::<()>::of(2),
            Shared::<()>::of(3),
        )
        .subscribe(|next| value = Some(next));

        assert_eq!(value, Some((1, 2, 3)));
    }

    #[test]
    fn maps_combined_tuple() {
        let mut value = None;

        crate::combine_latest!(
            Shared::<()>::of(1),
            Shared::<()>::of(2),
            Shared::<()>::of(3) => |(a, b, c)| a + b + c,
        )
        .subscribe(|next| value = Some(next));

        assert_eq!(value, Some(6));
    }

    #[test]
    fn supports_nine_sources() {
        let mut value = None;

        crate::combine_latest!(
            Shared::<()>::of(1),
            Shared::<()>::of(2),
            Shared::<()>::of(3),
            Shared::<()>::of(4),
            Shared::<()>::of(5),
            Shared::<()>::of(6),
            Shared::<()>::of(7),
            Shared::<()>::of(8),
            Shared::<()>::of(9) => |values| values,
        )
        .subscribe(|next| value = Some(next));

        assert_eq!(value, Some((1, 2, 3, 4, 5, 6, 7, 8, 9)));
    }
}
