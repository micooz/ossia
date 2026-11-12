export function documentIdFromLocation(): string | null {
  const documentId = new URLSearchParams(location.search).get('documentId');
  return documentId?.trim() || null;
}
