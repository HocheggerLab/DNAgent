// The only module that talks to the native shell. In the e2e Vite mode the same commands
// go to a live Rust test server (real desktop session, no recordings); production builds
// drop that branch.
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import type {
  DetectionResult, DocumentState, EnzymeCatalogueInfo, EnzymeCount, FeaturePreview, FeatureRequest, FileStamp, Fragment, HandoffItem, HandoffResult, SaveResult, Site, ViewReport,
} from './bindings';

const stub = import.meta.env.MODE === 'e2e' ? import('./testing/stub-backend') : null;
let pending = 0;

/** Native requests not yet settled; used by the test harness to wait deterministically. */
export const pendingRequests = () => pending;

async function tracked<T>(request: () => Promise<T>): Promise<T> {
  pending++;
  try { return await request(); } finally { pending--; }
}

function call<T>(command: string, args: Record<string, unknown>): Promise<T> {
  return tracked(async () => stub ? (await stub).invoke<T>(command, args) : invoke<T>(command, args));
}

export const openDocument = (path: string) => call<DocumentState>('open_document', { path });
export const previewFeature = (documentId: number, request: FeatureRequest) => call<FeaturePreview>('preview_feature', { documentId, request });
export const addFeature = (documentId: number, request: FeatureRequest) => call<DocumentState>('add_feature', { documentId, request });
export const removeFeature = (documentId: number, featureId: string) => call<DocumentState>('remove_feature', { documentId, featureId });
export const undo = (documentId: number) => call<DocumentState>('undo', { documentId });
export const redo = (documentId: number) => call<DocumentState>('redo', { documentId });
export const saveGenbank = (documentId: number, path: string) => call<SaveResult>('save_genbank', { documentId, path });
export const closeDocument = (documentId: number) => call<null>('close_document', { documentId });
export const reportView = (view: ViewReport) => call<null>('report_view', { view });
export const writeHandoff = (workspace: string, items: HandoffItem[]) => call<HandoffResult>('write_handoff', { workspace, items });
export const pollFiles = (workspace: string, openPaths: string[]) => call<FileStamp[]>('poll_files', { workspace, openPaths });
export const defaultWorkspace = () => call<string>('default_workspace', {});
export const detectFeatures = (documentId: number) => call<DetectionResult>('detect_features', { documentId });
export const addFeatures = (documentId: number, requests: FeatureRequest[]) => call<DocumentState>('add_features', { documentId, requests });
export const enzymeCatalogue = () => call<EnzymeCatalogueInfo>('enzyme_catalogue', {});
export const enzymeCounts = (documentId: number) => call<EnzymeCount[]>('enzyme_counts', { documentId });
export const findSites = (documentId: number, enzymes: string[]) => call<Site[]>('find_sites', { documentId, enzymes });
export const writeSvg = (path: string, svg: string) => call<null>('write_svg', { path, svg });
export const digest = (documentId: number, enzymes: string[]) => call<Fragment[]>('digest', { documentId, enzymes });

export function pickConstructPath(): Promise<string | null> {
  return tracked(async () => {
    if (stub) return (await stub).pickConstructPath();
    const path = await open({multiple:false,directory:false,filters:[{name:'DNA constructs',extensions:['dna','gb','gbk','genbank','fa','fasta','fna','json']}]});
    return typeof path === 'string' ? path : null;
  });
}

export function pickSavePath(defaultPath: string): Promise<string | null> {
  return tracked(async () => {
    if (stub) return (await stub).pickSavePath();
    const path = await save({ defaultPath, filters: [{ name: 'GenBank', extensions: ['gb', 'gbk', 'genbank'] }] });
    return path ?? null;
  });
}

export function pickSvgPath(defaultPath: string): Promise<string | null> {
  return tracked(async () => {
    if (stub) return (await stub).pickSavePath();
    const path = await save({ defaultPath, filters: [{ name: 'SVG drawing', extensions: ['svg'] }] });
    return path ?? null;
  });
}

export function pickWorkspace(defaultPath: string): Promise<string | null> {
  return tracked(async () => {
    if (stub) return (await stub).pickConstructPath();
    const path = await open({ multiple: false, directory: true, defaultPath });
    return typeof path === 'string' ? path : null;
  });
}
