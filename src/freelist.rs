//! Defines the [`Freelist`] struct and associated constants and functions.

use core::mem::{align_of, size_of};
use core::ptr::{addr_of_mut, null_mut, NonNull};

use super::header::HEADER_ALIGN;

pub const NODE_SIZE: usize = size_of::<Node>();
pub const NODE_ALIGN: usize = align_of::<Node>();

#[repr(C)]
pub struct Node {
    pub next: *mut Node,
    pub prev: *mut Node,
}

#[derive(Debug)]
#[repr(C)]
pub struct Freelist {
    head: *mut Node,
}

impl Freelist {
    /// Creates an empty Freelist.
    #[inline]
    pub const fn new() -> Self {
        Freelist { head: null_mut() }
    }

    /// Creates a node at the location pointed by `p` and adds it to the front of the Freelist.
    /// This operation has a time complexity of *O*(1).
    ///
    /// # Safety
    /// This function is unsafe since it assumes that `p` does not point to a real node
    /// but the place where the node is to be put, that is after the header of a block
    /// which is in the process of being freed. (so it's neither occupied nor free).
    pub unsafe fn push_front(&mut self, p: *mut Node) {
        let head: *mut *mut Node = addr_of_mut!(self.head);

        debug_assert_eq!((*head) as usize % NODE_ALIGN, 0);
        debug_assert_eq!((*head) as usize % HEADER_ALIGN, 0);

        p.write(Node {
            next: *head,
            prev: null_mut(),
        });
        if !(*head).is_null() {
            (**head).prev = p;
        }
        (*head) = p;
    }

    /// Removes `node` from the list.
    /// This operation has a time complexity of *O*(1).
    ///
    /// Safety:
    /// This function is unsafe since it assumes that `node` is part of the list.
    pub unsafe fn remove(&mut self, node: *const Node) {
        let prev = (*node).prev;
        let next = (*node).next;
        match prev.is_null() {
            true => self.head = next,
            false => (*prev).next = next,
        }
        if !next.is_null() {
            (*next).prev = prev;
        }
    }

    /// Returns the head of the list or `None` if the list is empty.
    /// This operation has a time complexity of *O*(1).
    #[inline]
    pub fn head(&self) -> Option<NonNull<Node>> {
        NonNull::new(self.head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::MaybeUninit;

    #[test]
    fn test_1() {
        assert!(Freelist::new().head().is_none(), "List should be empty");
    }

    #[test]
    fn test_2() {
        let mut list = Freelist::new();
        let count = 1000;

        let mut nodes: Vec<MaybeUninit<Node>> = (0..count).map(|_| MaybeUninit::uninit()).collect();

        for i in 0..count {
            unsafe {
                list.push_front(nodes[i].as_mut_ptr());
            }
        }

        for i in (0..count).rev() {
            let Some(head) = list.head() else {
                panic!("List should not be empty.");
            };
            let head = head.as_ptr();

            unsafe {
                assert_eq!(
                    head as usize,
                    nodes.as_ptr().add(i) as usize,
                    "The list head should be placed at nodes[{i}]."
                );
                list.remove(head);
            }
        }
    }

    #[test]
    fn test_3() {
        let mut list = Freelist::new();

        let count = 20;
        let mut nodes: Vec<MaybeUninit<Node>> = (0..count).map(|_| MaybeUninit::uninit()).collect();

        for i in 0..count {
            unsafe {
                list.push_front(nodes[i].as_mut_ptr());
            }
        }

        for i in 1..count - 1 {
            assert!(list.head().is_some(), "List should not be empty.");

            unsafe {
                list.remove(nodes.as_ptr().add(i).cast());
            }
        }

        let head = list.head().unwrap().as_ptr();

        unsafe {
            assert_eq!(
                head as usize,
                nodes.as_ptr().add(count - 1) as usize,
                "The list head should be placed at nodes[{}].",
                count - 1
            );
            list.remove(head);
        }

        let head = list.head().unwrap().as_ptr();

        unsafe {
            assert_eq!(
                head as usize,
                nodes.as_ptr() as usize,
                "The list head should be placed at nodes[0]"
            );
            list.remove(head);
        }
    }

    #[test]
    fn test_4() {
        let mut list = Freelist::new();

        let count = 200;
        let mut nodes: Vec<MaybeUninit<Node>> = (0..count).map(|_| MaybeUninit::uninit()).collect();

        for i in 0..count {
            unsafe {
                list.push_front(nodes[i].as_mut_ptr());
            }
        }

        let mut p: *mut Node = list.head().unwrap().as_ptr();

        while !p.is_null() {
            unsafe {
                if (*p).next.is_null() {
                    assert_eq!(p, nodes.as_mut_ptr().cast());
                } else {
                    assert_eq!((*p).next, p.sub(1));
                }
                if (*p).prev.is_null() {
                    assert_eq!(p, nodes.as_mut_ptr().add(count - 1).cast())
                } else {
                    assert!((*p).prev == p.add(1));
                }
                p = (*p).next;
            }
        }
    }
}
