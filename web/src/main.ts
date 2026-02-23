import './style.css'
import { router, routes, Route } from './router'
import { renderDashboard, initDashboard, cleanupDashboard } from './pages/Dashboard'
import { renderReviewQueue, initReviewQueue, cleanupReviewQueue } from './pages/ReviewQueue'
import { renderOrganizeSuggestions, initOrganizeSuggestions, cleanupOrganizeSuggestions } from './pages/OrganizeSuggestions'
import { renderOrphans, initOrphans, cleanupOrphans } from './pages/Orphans'
import { initKeyboardNavigation, resetFocus } from './keyboard'
import { initToasts } from './components/Toast'

// Mobile menu state
let mobileMenuOpen = false;

function renderPage(route: Route): string {
  switch (route) {
    case '/': return renderDashboard();
    case '/review': return renderReviewQueue();
    case '/organize': return renderOrganizeSuggestions();
    case '/orphans': return renderOrphans();
    default: return renderDashboard();
  }
}

// Desktop navigation component
function renderDesktopNav(currentRoute: Route): string {
  return routes.map(r => {
    const isActive = r.path === currentRoute;
    const baseClasses = 'flex items-center space-x-2 px-3 py-2 rounded-md text-sm font-medium transition-colors';
    const activeClasses = isActive
      ? 'bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-200'
      : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700';
    return `<a href="#${r.path}" class="${baseClasses} ${activeClasses}">
      <span>${r.icon}</span>
      <span>${r.title}</span>
    </a>`;
  }).join('');
}

// Mobile navigation component
function renderMobileNav(currentRoute: Route): string {
  return routes.map(r => {
    const isActive = r.path === currentRoute;
    const baseClasses = 'flex items-center space-x-3 px-4 py-3 text-base font-medium transition-colors';
    const activeClasses = isActive
      ? 'bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-200'
      : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700';
    return `<a href="#${r.path}" class="mobile-nav-link ${baseClasses} ${activeClasses}">
      <span class="text-xl">${r.icon}</span>
      <span>${r.title}</span>
    </a>`;
  }).join('');
}

// Hamburger menu icon
function renderHamburgerIcon(): string {
  return `
    <button id="mobile-menu-btn" class="md:hidden p-2 rounded-md text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700" aria-label="${mobileMenuOpen ? 'Close menu' : 'Open menu'}" aria-expanded="${mobileMenuOpen}" aria-controls="mobile-nav">
      <svg class="w-6 h-6 ${mobileMenuOpen ? 'hidden' : 'block'}" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16"></path>
      </svg>
      <svg class="w-6 h-6 ${mobileMenuOpen ? 'block' : 'hidden'}" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path>
      </svg>
    </button>
  `;
}

// Track current route for cleanup
let currentRoute: Route | null = null;

// Cleanup previous page
function cleanupPage(route: Route | null): void {
  if (route === '/' || route === null) {
    cleanupDashboard();
  } else if (route === '/review') {
    cleanupReviewQueue();
  } else if (route === '/organize') {
    cleanupOrganizeSuggestions();
  } else if (route === '/orphans') {
    cleanupOrphans();
  }
}

// Initialize current page
function initPage(route: Route): void {
  if (route === '/') {
    initDashboard();
  } else if (route === '/review') {
    initReviewQueue();
  } else if (route === '/organize') {
    initOrganizeSuggestions();
  } else if (route === '/orphans') {
    initOrphans();
  }
  
  // Reset keyboard focus state after page init
  resetFocus();
}

// App shell
function renderApp(route: Route): void {
  // Cleanup previous page
  cleanupPage(currentRoute);
  currentRoute = route;
  // Close mobile menu on navigation
  mobileMenuOpen = false;

  const pageTitle = routes.find(r => r.path === route)?.title || 'Dashboard';

  const app = document.querySelector<HTMLDivElement>('#app')!;
  app.innerHTML = `
    <a href="#main-content" class="skip-link">Skip to main content</a>
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900">
      <header class="bg-white dark:bg-gray-800 shadow" role="banner">
        <div class="max-w-7xl mx-auto px-4 py-4">
          <div class="flex items-center justify-between">
            <a href="#/" class="text-xl font-bold text-blue-600 dark:text-blue-400" aria-label="Factbase - Go to dashboard">Factbase</a>
            <!-- Desktop navigation -->
            <nav class="hidden md:flex space-x-1" role="navigation" aria-label="Main navigation">
              ${renderDesktopNav(route)}
            </nav>
            <!-- Mobile menu button -->
            ${renderHamburgerIcon()}
          </div>
        </div>
        <!-- Mobile navigation -->
        <nav id="mobile-nav" class="md:hidden ${mobileMenuOpen ? 'block' : 'hidden'} border-t border-gray-200 dark:border-gray-700" role="navigation" aria-label="Mobile navigation">
          <div class="px-2 py-2 space-y-1">
            ${renderMobileNav(route)}
          </div>
        </nav>
      </header>
      <main id="main-content" class="max-w-7xl mx-auto px-4 py-6 sm:py-8" role="main" aria-label="${pageTitle}">
        ${renderPage(route)}
      </main>
    </div>
  `;

  // Set up mobile menu toggle
  setupMobileMenu();

  // Initialize current page
  initPage(route);
}

// Set up mobile menu toggle handler
function setupMobileMenu(): void {
  const menuBtn = document.getElementById('mobile-menu-btn');
  menuBtn?.addEventListener('click', () => {
    mobileMenuOpen = !mobileMenuOpen;
    const mobileNav = document.getElementById('mobile-nav');
    if (mobileNav) {
      mobileNav.classList.toggle('hidden', !mobileMenuOpen);
    }
    // Update button aria-expanded and icons
    if (menuBtn) {
      menuBtn.setAttribute('aria-expanded', String(mobileMenuOpen));
      const icons = menuBtn.querySelectorAll('svg');
      icons[0]?.classList.toggle('hidden', mobileMenuOpen);
      icons[0]?.classList.toggle('block', !mobileMenuOpen);
      icons[1]?.classList.toggle('hidden', !mobileMenuOpen);
      icons[1]?.classList.toggle('block', mobileMenuOpen);
    }
  });

  // Close mobile menu when clicking a nav link
  document.querySelectorAll('.mobile-nav-link').forEach(link => {
    link.addEventListener('click', () => {
      mobileMenuOpen = false;
    });
  });
}

// Initialize app
router.onRouteChange((route) => {
  renderApp(route);
});

// Initialize keyboard navigation once at app start
initKeyboardNavigation();

// Initialize toast container
initToasts();

// Initial render
renderApp(router.getCurrentRoute());
