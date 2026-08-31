// Scroll-lock for modal drawers: hides the page scrollbar while a modal is open so the
// modal's own scrollbar is the only one (html has `scrollbar-gutter: stable`, so no layout
// shift). Counted, so stacked modals behave.
let count = 0;

export const lockScroll = (): void => {
  count += 1;
  if (count === 1) document.documentElement.style.overflow = 'hidden';
};

export const unlockScroll = (): void => {
  count = Math.max(0, count - 1);
  if (count === 0) document.documentElement.style.overflow = '';
};
