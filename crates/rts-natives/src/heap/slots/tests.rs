//! Behaviour-equivalence tests for the two [`Slots`] forms.

use super::{INLINE_CAP, Slots};



    #[test]
    /// The two forms are observationally identical for every accessor — this is
    /// the property the whole change rests on.
    fn both_forms_agree_on_every_accessor() {
        let mut inline = Slots::with_capacity_inline(4);
        let mut heap = Slots::from_vec(Vec::new());
        assert!(inline.is_inline() && !heap.is_inline());

        for (a, b) in [(&mut inline, &mut heap)] {
            for v in [10i64, 20, 30] {
                a.push(v);
                b.push(v);
            }
        }
        assert_eq!(inline.as_slice(), heap.as_slice());
        assert_eq!(inline.len(), 3);

        inline.resize(6, -1);
        heap.resize(6, -1);
        assert_eq!(inline.as_slice(), heap.as_slice());

        inline.insert(1, 99);
        heap.insert(1, 99);
        assert_eq!(inline.as_slice(), heap.as_slice());

        assert_eq!(inline.remove(1), heap.remove(1));
        assert_eq!(inline.pop(), heap.pop());
        inline.truncate(2);
        heap.truncate(2);
        assert_eq!(inline.as_slice(), heap.as_slice());
        assert_eq!(inline.to_owned_vec(), heap.to_owned_vec());
    }

    #[test]
    /// Growth past the capacity promotes, and the words survive intact.
    fn push_past_capacity_promotes_and_preserves_words() {
        let mut s = Slots::with_capacity_inline(2);
        for i in 0..(INLINE_CAP as i64) {
            s.push(i);
        }
        assert!(s.is_inline(), "exactly INLINE_CAP words must still be inline");
        s.push(777);
        assert!(!s.is_inline(), "one more word must promote");
        let expect: Vec<i64> = (0..INLINE_CAP as i64).chain([777]).collect();
        assert_eq!(s.as_slice(), expect.as_slice());
    }

    #[test]
    /// `resize` past the capacity promotes and fills with the requested word —
    /// this is the JS sparse-array HOLE path.
    fn resize_past_capacity_promotes() {
        let mut s = Slots::with_capacity_inline(1);
        s.push(1);
        s.resize(INLINE_CAP + 4, -7);
        assert!(!s.is_inline());
        assert_eq!(s.len(), INLINE_CAP + 4);
        assert_eq!(s[0], 1);
        assert_eq!(s[INLINE_CAP + 3], -7);
    }

    #[test]
    /// Promotion is one-way: nothing demotes a heap value back into the block,
    /// so a guard on the form only has to be checked once per observation.
    fn promotion_is_one_way() {
        let mut s = Slots::with_capacity_inline(4);
        s.push(1);
        s.push(2);
        s.promote();
        assert!(!s.is_inline());
        s.clear();
        s.push(3);
        assert!(!s.is_inline(), "clearing must not demote");
    }

    #[test]
    /// A value built from a `Vec` is HEAP — arrays never take the block.
    fn from_vec_is_always_heap() {
        assert!(!Slots::from_vec(vec![1, 2]).is_inline());
        assert!(!Slots::with_capacity_inline(INLINE_CAP + 1).is_inline());
    }

    #[test]
    fn retain_and_extend_agree_across_forms() {
        let mut a = Slots::with_capacity_inline(8);
        let mut b = Slots::from_vec(Vec::new());
        a.extend_from_slice(&[1, 2, 3, 4]);
        b.extend_from_slice(&[1, 2, 3, 4]);
        a.retain(|x| x % 2 == 0);
        b.retain(|x| x % 2 == 0);
        assert_eq!(a.as_slice(), b.as_slice());
        assert_eq!(a.as_slice(), &[2, 4]);
    }
