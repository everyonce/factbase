// Simple hash-based router for SPA navigation

export type Route = '/' | '/review' | '/organize' | '/orphans';

export interface RouteConfig {
  path: Route;
  title: string;
  icon: string;
}

export const routes: RouteConfig[] = [
  { path: '/', title: 'Dashboard', icon: '📊' },
  { path: '/review', title: 'Review Queue', icon: '❓' },
  { path: '/organize', title: 'Organize', icon: '📁' },
  { path: '/orphans', title: 'Orphans', icon: '📝' },
];

export type RouteChangeHandler = (route: Route) => void;

class Router {
  private handlers: RouteChangeHandler[] = [];

  constructor() {
    window.addEventListener('hashchange', () => this.handleChange());
    window.addEventListener('load', () => this.handleChange());
  }

  private handleChange(): void {
    const route = this.getCurrentRoute();
    this.handlers.forEach(handler => handler(route));
  }

  getCurrentRoute(): Route {
    const hash = window.location.hash.slice(1) || '/';
    const validRoutes: Route[] = ['/', '/review', '/organize', '/orphans'];
    return validRoutes.includes(hash as Route) ? (hash as Route) : '/';
  }

  navigate(route: Route): void {
    window.location.hash = route;
  }

  onRouteChange(handler: RouteChangeHandler): void {
    this.handlers.push(handler);
  }
}

export const router = new Router();
