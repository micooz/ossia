export function documentIdFromLocation(): string | null {
  const documentId = new URLSearchParams(location.search).get('documentId');
  return documentId?.trim() || null;
}

export function setDocumentIdInLocation(documentId: string | null) {
  const url = new URL(location.href);
  url.hash = '';
  if (documentId) url.searchParams.set('documentId', documentId);
  else url.searchParams.delete('documentId');
  history.replaceState(null, '', `${url.pathname}${url.search}`);
}
