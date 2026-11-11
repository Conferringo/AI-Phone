import {redirect} from 'next/navigation';

// Keep old bookmarks working without a separate entry screen.
export default function LegacyEntry(){redirect('/phone');}
