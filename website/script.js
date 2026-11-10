const menuToggle = document.querySelector('.menu-toggle');
const mobileNav = document.querySelector('#mobile-nav');
menuToggle.hidden = false;

function closeMenu() {
  mobileNav.hidden = true;
  menuToggle.setAttribute('aria-expanded', 'false');
  menuToggle.setAttribute('aria-label', 'Open navigation');
}

menuToggle.addEventListener('click', () => {
  const opening = mobileNav.hidden;
  mobileNav.hidden = !opening;
  menuToggle.setAttribute('aria-expanded', String(opening));
  menuToggle.setAttribute('aria-label', opening ? 'Close navigation' : 'Open navigation');
});
mobileNav.addEventListener('click', event => {
  if (event.target.closest('a')) closeMenu();
});
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && !mobileNav.hidden) {
    closeMenu();
    menuToggle.focus();
  }
});
const desktop = window.matchMedia('(min-width: 761px)');
desktop.addEventListener('change', event => { if (event.matches) closeMenu(); });

const exampleSwitch = document.querySelector('.example-switch');
const textExample = document.querySelector('[data-text-example]');
const imageExample = document.querySelector('[data-image-example]');
exampleSwitch.hidden = false;
exampleSwitch.addEventListener('click', event => {
  const button = event.target.closest('[data-example]');
  if (!button || button.getAttribute('aria-pressed') === 'true') return;
  const showImage = button.dataset.example === 'image';
  textExample.hidden = showImage;
  imageExample.hidden = !showImage;
  for (const option of exampleSwitch.querySelectorAll('button')) {
    option.setAttribute('aria-pressed', String(option === button));
  }
});
