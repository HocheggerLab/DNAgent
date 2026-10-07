// Feature ordering for a gene locus. The sidebar's default is annotation order, which for
// a locus bundle buries the transcript the user cares about among thirty others sorted by
// accession. Here the quantifier panel's ranking (most expressed first, computed in Rust)
// drives the list, so the abundant transcript is at the top.
import type { Feature, LocusView } from './bindings';

/** The feature ids of one transcript, mRNA first, in the order they should be listed. */
function transcriptFeatures(locus: LocusView, transcriptId: string): string[] {
  const isoform = locus.isoforms.find(i => i.transcript_id === transcriptId);
  if (!isoform) return [];
  return isoform.cds_feature_id === null
    ? [isoform.mrna_feature_id]
    : [isoform.mrna_feature_id, isoform.cds_feature_id];
}

/** Every feature id the bundle claims for a transcript. */
function transcriptOwned(locus: LocusView): Set<string> {
  const owned = new Set<string>();
  for (const isoform of locus.isoforms) {
    owned.add(isoform.mrna_feature_id);
    if (isoform.cds_feature_id !== null) owned.add(isoform.cds_feature_id);
  }
  return owned;
}

/**
 * `features` reordered by the ranking of `quantifier`: anything belonging to no transcript
 * first (whatever the user added, which they are looking for), then every transcript in
 * that panel's order as mRNA then CDS, then any transcript the panel does not rank.
 * Returns the input unchanged when the bundle has no such panel — a bundle without
 * expression data has no ranking to apply, and the isoform view says so.
 */
export function orderedFeatures(
  features: readonly Feature[],
  locus: LocusView,
  quantifier: string | null,
): Feature[] {
  const panel = locus.quantifiers.find(p => p.quantifier === quantifier);
  if (!panel) return [...features];
  const byId = new Map(features.map(f => [f.id, f]));
  const owned = transcriptOwned(locus);
  const ranked: Feature[] = [];
  const placed = new Set<string>();
  for (const transcript of panel.order) {
    for (const id of transcriptFeatures(locus, transcript)) {
      const feature = byId.get(id);
      // A transcript whose features were deleted is reported as `missing_features`.
      if (feature && !placed.has(id)) { ranked.push(feature); placed.add(id); }
    }
  }
  const own = features.filter(f => !owned.has(f.id));
  const unranked = features.filter(f => owned.has(f.id) && !placed.has(f.id));
  return [...own, ...ranked, ...unranked];
}
