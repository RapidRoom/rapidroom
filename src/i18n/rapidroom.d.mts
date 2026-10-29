import type { Resource } from 'i18next';

export function deepMerge<T, U>(upstream: T, overlay: U): T & U;
export function withRapidRoomLocales(resources: Resource): Resource;
